//! The client side of the hub: connections, requests and subscriptions.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use parking_lot::Mutex;
use protocol::PROTOCOL_VERSION;
use protocol::client::{
    ApiError, ClientFrame, ClientRequest, ClientResponse, ErrorCode, HubFrame, HubNotice, ReplyOutcome,
};
use protocol::frame;
use protocol::model::{Seq, SessionId};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};
use tokio::task::AbortHandle;
use tracing::{debug, warn};

use crate::Hub;
use crate::session::{LiveItem, LiveSession};

pub(crate) const DEFAULT_READ_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const MAX_READ_EVENTS: u32 = 2000;
pub(crate) const PUMP_PAGE_EVENTS: u32 = 256;

/// Bounded, so a client that stops reading holds back only its own subscriptions.
const CONNECTION_QUEUE: usize = 1024;

pub(crate) async fn accept_loop(hub: Arc<Hub>, listener: UnixListener) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(serve_connection(hub.clone(), stream));
            }
            Err(e) => {
                warn!("accepting a connection failed: {e}");
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

struct Connection {
    frames: mpsc::Sender<HubFrame>,
    subscriptions: Mutex<HashMap<u64, AbortHandle>>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        for (_, subscription) in self.subscriptions.lock().drain() {
            subscription.abort();
        }
    }
}

async fn serve_connection(hub: Arc<Hub>, stream: UnixStream) {
    hub.clients.fetch_add(1, Ordering::Relaxed);
    let (read, mut write) = stream.into_split();
    let (frames_tx, mut frames_rx) = mpsc::channel::<HubFrame>(CONNECTION_QUEUE);
    let writer = tokio::spawn(async move {
        while let Some(item) = frames_rx.recv().await {
            let Ok(line) = frame::encode(&item) else { continue };
            if write.write_all(&line).await.is_err() {
                return;
            }
        }
    });
    let connection = Arc::new(Connection { frames: frames_tx, subscriptions: Mutex::new(HashMap::new()) });

    let mut reader = BufReader::new(read);
    let mut line = Vec::new();
    let mut greeted = false;
    loop {
        line.clear();
        let read = (&mut reader).take(frame::MAX_FRAME_BYTES as u64 + 1).read_until(b'\n', &mut line).await;
        match read {
            Ok(0) | Err(_) => break,
            Ok(_) if line.len() > frame::MAX_FRAME_BYTES => break,
            Ok(_) => {}
        }
        let request: ClientFrame = match frame::decode(&line) {
            Ok(request) => request,
            Err(e) => {
                debug!("closing a connection that sent a bad frame: {e}");
                break;
            }
        };
        if !greeted {
            let (outcome, version_ok) = hello(&request.request);
            let _ = connection.frames.send(HubFrame::Reply { id: request.id, outcome }).await;
            if !version_ok {
                break;
            }
            greeted = true;
            continue;
        }
        let hub = hub.clone();
        let connection = connection.clone();
        tokio::spawn(async move {
            let outcome = match handle(&hub, &connection, request.request).await {
                Ok(response) => ReplyOutcome::Response(response),
                Err(error) => ReplyOutcome::Error(error),
            };
            let _ = connection.frames.send(HubFrame::Reply { id: request.id, outcome }).await;
        });
    }
    drop(connection);
    // Finish writing what's queued, such as the error for a version mismatch.
    let _ = writer.await;
    hub.clients.fetch_sub(1, Ordering::Relaxed);
}

fn hello(request: &ClientRequest) -> (ReplyOutcome, bool) {
    let error = |message: String| (ReplyOutcome::Error(ApiError::new(ErrorCode::VersionMismatch, message)), false);
    let ClientRequest::Hello { protocol_version, .. } = request else {
        return error("the first request on a connection must be hello".to_string());
    };
    if *protocol_version == PROTOCOL_VERSION {
        (
            ReplyOutcome::Response(ClientResponse::Hello {
                protocol_version: PROTOCOL_VERSION,
                hub_version: env!("CARGO_PKG_VERSION").to_string(),
            }),
            true,
        )
    } else {
        error(format!(
            "this hub speaks protocol {PROTOCOL_VERSION}, and the client speaks {protocol_version}. Restart the hub with the same version"
        ))
    }
}

async fn handle(
    hub: &Arc<Hub>,
    connection: &Arc<Connection>,
    request: ClientRequest,
) -> Result<ClientResponse, ApiError> {
    use ClientRequest as R;
    use ClientResponse as C;
    Ok(match request {
        R::Hello { .. } => return Err(ApiError::new(ErrorCode::InvalidRequest, "already said hello")),
        R::HubStatus => C::HubStatus(hub.status()),
        R::HubShutdown => {
            hub.request_shutdown();
            C::Ok
        }
        R::RepoAdd { path } => C::Repo(hub.repo_add(path).await?),
        R::RepoList => C::Repos { repos: hub.repo_list().await? },
        R::WorkspaceCreate { repo_id, name, base_ref } => {
            C::Workspace(hub.workspace_create(repo_id, name, base_ref).await?)
        }
        R::WorkspaceList { include_archived } => {
            C::Workspaces { workspaces: hub.workspace_list(include_archived).await? }
        }
        R::WorkspaceArchive { id, force } => C::Workspace(hub.workspace_archive(id, force).await?),
        R::WorkspaceRestore { id } => C::Workspace(hub.workspace_restore(id).await?),
        R::WorkspaceStatus { id } => C::GitStatus(hub.workspace_status(id).await?),
        R::WorkspaceDiff { id } => C::GitDiff(hub.workspace_diff(id).await?),
        R::WorkspaceSetupLog { id } => C::SetupLog { text: hub.workspace_setup_log(id).await? },
        R::WorkspaceRerunSetup { id } => C::Workspace(hub.workspace_rerun_setup(id).await?),
        R::WorkspacePullRequest { id } => C::PullRequest { pull_request: hub.workspace_pull_request(id).await? },
        R::CommentAdd { workspace_id, path, side, line, text } => {
            C::Comment(hub.comment_add(workspace_id, path, side, line, text).await?)
        }
        R::CommentList { workspace_id } => C::Comments { comments: hub.comment_list(workspace_id).await? },
        R::CommentDelete { id } => {
            hub.comment_delete(id).await?;
            C::Ok
        }
        R::SessionCreate { workspace_id, agent, model, permission_mode } => {
            C::Session(hub.session_create(workspace_id, agent, model, permission_mode).await?)
        }
        R::SessionEnsure { workspace_id, agent } => C::Session(hub.session_ensure(workspace_id, agent).await?),
        R::SessionList { workspace_id } => C::Sessions { sessions: hub.session_list(workspace_id).await? },
        R::SessionGet { id } => C::Session(hub.session_get(id).await?),
        R::SessionRead { id, from_seq, limit, max_bytes, include_raw } => {
            hub.session_read(id, from_seq, limit, max_bytes, include_raw).await?
        }
        R::SessionSubscribe { id, from_seq, subscription_id } => {
            let session = hub.live_session(&id).await?;
            let mut subscriptions = connection.subscriptions.lock();
            if subscriptions.contains_key(&subscription_id) {
                return Err(ApiError::new(
                    ErrorCode::InvalidRequest,
                    format!("subscription {subscription_id} already exists"),
                ));
            }
            let live = session.subscribe();
            let task = tokio::spawn(pump(
                hub.clone(),
                session,
                id,
                subscription_id,
                from_seq,
                live,
                connection.frames.clone(),
            ));
            subscriptions.insert(subscription_id, task.abort_handle());
            C::Ok
        }
        R::FeedSubscribe { subscription_id } => {
            let mut subscriptions = connection.subscriptions.lock();
            if subscriptions.contains_key(&subscription_id) {
                return Err(ApiError::new(
                    ErrorCode::InvalidRequest,
                    format!("subscription {subscription_id} already exists"),
                ));
            }
            let task = tokio::spawn(pump_feed(hub.subscribe_feed(), subscription_id, connection.frames.clone()));
            subscriptions.insert(subscription_id, task.abort_handle());
            C::Ok
        }
        R::SessionUnsubscribe { subscription_id } => {
            if let Some(subscription) = connection.subscriptions.lock().remove(&subscription_id) {
                subscription.abort();
            }
            C::Ok
        }
        R::SessionPrompt { id, text, client_msg_id } => {
            let (seq, duplicate) = hub.session_prompt(id, text, client_msg_id).await?;
            C::PromptRecorded { seq, duplicate }
        }
        R::SessionApprove { id, approval_id, decision } => {
            hub.session_approve(id, approval_id, decision).await?;
            C::Ok
        }
        R::SessionConfigure { id, model, permission_mode } => {
            C::Session(hub.session_configure(id, model, permission_mode).await?)
        }
        R::SessionClose { id } => C::Session(hub.session_close(id).await?),
        R::SessionReopen { id } => C::Session(hub.session_reopen(id).await?),
        R::SessionCancel { id } => {
            hub.session_cancel(id).await?;
            C::Ok
        }
    })
}

async fn pump_feed(mut feed: broadcast::Receiver<HubNotice>, subscription_id: u64, frames: mpsc::Sender<HubFrame>) {
    loop {
        let notice = match feed.recv().await {
            Ok(notice) => notice,
            Err(broadcast::error::RecvError::Lagged(_)) => HubNotice::Resync,
            Err(broadcast::error::RecvError::Closed) => return,
        };
        if frames.send(HubFrame::Notice { subscription_id, notice }).await.is_err() {
            return;
        }
    }
}

/// Feeds one subscription. The log is always the source of durable events: live items only say
/// that the log has moved, so a slow client can fall behind and catch up without losing anything.
async fn pump(
    hub: Arc<Hub>,
    session: Arc<LiveSession>,
    id: SessionId,
    subscription_id: u64,
    from_seq: Seq,
    mut live: broadcast::Receiver<crate::session::Live>,
    frames: mpsc::Sender<HubFrame>,
) {
    let mut cursor = from_seq.max(1);
    'resync: loop {
        let snapshot = session.snapshot();
        if send_log_through(&hub, &id, subscription_id, &mut cursor, snapshot.last_seq, &frames).await.is_err() {
            return;
        }
        if frames.send(HubFrame::Tail { subscription_id, tail: snapshot.tail }).await.is_err() {
            return;
        }
        loop {
            let item = match live.recv().await {
                Ok(item) => item,
                // Missed live items. The log has every durable event, and a new snapshot replaces
                // the tail, so this loses nothing.
                Err(broadcast::error::RecvError::Lagged(_)) => continue 'resync,
                Err(broadcast::error::RecvError::Closed) => return,
            };
            if item.version <= snapshot.version {
                continue;
            }
            let sent = match item.item {
                LiveItem::Appended(seq) => {
                    send_log_through(&hub, &id, subscription_id, &mut cursor, seq, &frames).await
                }
                LiveItem::Delta(delta) => frames.send(HubFrame::Delta { subscription_id, delta }).await.map_err(|_| ()),
            };
            if sent.is_err() {
                return;
            }
        }
    }
}

async fn send_log_through(
    hub: &Arc<Hub>,
    id: &str,
    subscription_id: u64,
    cursor: &mut Seq,
    through: Seq,
    frames: &mpsc::Sender<HubFrame>,
) -> Result<(), ()> {
    while *cursor <= through {
        // Stopping at `through` matters. An event read early would overtake the live deltas that came
        // before it, and those would then land on a tail the event had already cleared.
        let page = hub.read_page(id.to_string(), *cursor, through).await.map_err(|e| {
            warn!(session = id, "reading the log for a subscriber failed: {e}");
        })?;
        if page.is_empty() {
            return Ok(());
        }
        for event in page {
            *cursor = event.seq + 1;
            frames.send(HubFrame::Event { subscription_id, event }).await.map_err(|_| ())?;
        }
    }
    Ok(())
}
