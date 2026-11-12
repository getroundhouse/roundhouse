//! The client side of the hub: connections, requests and subscriptions.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use parking_lot::Mutex;
use protocol::Build;
use protocol::client::{
    ApiError, ClientRequest, ClientResponse, ErrorCode, HubFrame, HubNotice, ReplyOutcome, drafts, files, queue, waits,
};
use protocol::frame::{self, Call};
use protocol::model::{Place, SessionId};
use protocol::versions::Versions;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc};
use tokio::task::AbortHandle;
use tracing::{debug, warn};

use crate::attachment_files::FileFor;
use crate::frames::FrameReader;
use crate::hub::Hub;
use crate::listener::Listener;
use crate::store::AddedBy;
use crate::versions;
use pump::{Following, pump};

mod pump;

pub(crate) const DEFAULT_READ_BYTES: u64 = 4 * 1024 * 1024;
pub(crate) const MAX_READ_EVENTS: u32 = 2000;
pub(crate) const PUMP_PAGE_EVENTS: u32 = 256;

/// Bounded, so a client that stops reading holds back only its own subscriptions.
const CONNECTION_QUEUE: usize = 1024;

pub(crate) async fn accept_loop(hub: Arc<Hub>, mut listener: Listener) {
    loop {
        match listener.accept().await {
            Ok((read, write)) => {
                tokio::spawn(serve_connection(hub.clone(), read, write));
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

async fn serve_connection(
    hub: Arc<Hub>,
    read: impl AsyncRead + Unpin,
    mut write: impl AsyncWrite + Unpin + Send + 'static,
) {
    hub.clients.fetch_add(1, Ordering::Relaxed);
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

    let mut incoming = FrameReader::new(read);
    let mut greeted = false;
    loop {
        let Ok(Some(line)) = incoming.next().await else { break };
        // A request this hub can't read is a newer client's, which is refused once it's past the hello, as one this
        // hub doesn't know is.
        let Call { id, request } = match frame::read_call::<ClientRequest>(line) {
            Ok(call) => call,
            Err(e) => {
                debug!("closing a connection that sent a bad frame: {e}");
                break;
            }
        };
        if !greeted && !request.as_ref().is_ok_and(works_without_hello) {
            let (outcome, version_ok) = hello(hub.build(), request.as_ref().ok());
            let _ = connection.frames.send(HubFrame::Reply { id, outcome }).await;
            if !version_ok {
                break;
            }
            greeted = true;
            continue;
        }
        let hub = hub.clone();
        let connection = connection.clone();
        // Counted as it's read, rather than once its task starts, so a request to stop if idle sees every request this
        // connection sent before it, even one whose task hasn't started.
        hub.requests.fetch_add(1, Ordering::Relaxed);
        tokio::spawn(async move {
            let answered = match request {
                Ok(request) => handle(&hub, &connection, request).await,
                Err(e) => Err(hub.unsupported(Some(&e))),
            };
            let outcome = match answered {
                Ok(response) => ReplyOutcome::Response(response),
                Err(error) => ReplyOutcome::Error(error),
            };
            hub.requests.fetch_sub(1, Ordering::Relaxed);
            let _ = connection.frames.send(HubFrame::Reply { id, outcome }).await;
        });
    }
    drop(connection);
    // Finish writing what's queued, such as the error for a version mismatch.
    let _ = writer.await;
    hub.clients.fetch_sub(1, Ordering::Relaxed);
}

/// What a client of any version can do, so that a hub it can't otherwise talk to can still be seen and
/// stopped. Anyone who can reach this socket is this user, as `bind_socket` makes sure. A connection from
/// elsewhere, once there are any, shouldn't get this, because stopping a shared hub is for its admins.
fn works_without_hello(request: &ClientRequest) -> bool {
    matches!(request, ClientRequest::HubStatus | ClientRequest::HubShutdown | ClientRequest::HubRestart)
}

/// `None` for a first request this hub couldn't read, which can't be a hello it can answer.
fn hello(build: Build, request: Option<&ClientRequest>) -> (ReplyOutcome, bool) {
    let error = |message: String| (ReplyOutcome::Error(ApiError::new(ErrorCode::VersionMismatch, message)), false);
    let Some(ClientRequest::Hello { protocol_version, oldest_protocol_version, .. }) = request else {
        return error("the first request on a connection must be hello".to_string());
    };
    let theirs = Versions::from_hello(*protocol_version, *oldest_protocol_version);
    match versions::CLIENTS.newest_shared(theirs) {
        Some(chosen) => (
            ReplyOutcome::Response(ClientResponse::Hello {
                protocol_version: chosen,
                hub_version: build.version().to_string(),
            }),
            true,
        ),
        None => error(format!(
            "this hub is version {} on {}, and this client speaks {theirs}. Stopping the hub, which also stops its agents, lets one that matches start",
            build.version(),
            versions::CLIENTS,
        )),
    }
}

/// Holds the lock while `start` runs, so two requests can't both take one subscription ID.
fn add_subscription(
    connection: &Connection,
    subscription_id: u64,
    start: impl FnOnce() -> AbortHandle,
) -> Result<(), ApiError> {
    let mut subscriptions = connection.subscriptions.lock();
    if subscriptions.contains_key(&subscription_id) {
        return Err(ApiError::new(ErrorCode::InvalidRequest, format!("subscription {subscription_id} already exists")));
    }
    subscriptions.insert(subscription_id, start());
    Ok(())
}

/// Starts feeding a subscription to a session's log.
async fn subscribe(
    hub: &Arc<Hub>,
    connection: &Connection,
    id: SessionId,
    subscription_id: u64,
    following: Following,
) -> Result<ClientResponse, ApiError> {
    let session = hub.live_session(&id).await?;
    add_subscription(connection, subscription_id, || {
        let live = session.subscribe();
        let frames = connection.frames.clone();
        tokio::spawn(pump(hub.clone(), session.clone(), id, subscription_id, following, live, frames)).abort_handle()
    })?;
    Ok(ClientResponse::Ok)
}

#[expect(clippy::too_many_lines, reason = "a flat dispatch, with one short arm per request")]
async fn handle(
    hub: &Arc<Hub>,
    connection: &Arc<Connection>,
    request: ClientRequest,
) -> Result<ClientResponse, ApiError> {
    use ClientRequest as R;
    use ClientResponse as C;
    Ok(match request {
        R::Hello { .. } => return Err(ApiError::new(ErrorCode::InvalidRequest, "already said hello")),
        R::Unknown => return Err(hub.unsupported(None)),
        R::HubStatus => C::HubStatus(hub.status()),
        R::HubShutdown => {
            hub.request_stop();
            C::Ok
        }
        R::HubShutdownIfIdle => {
            hub.shutdown_if_idle()?;
            C::Ok
        }
        R::HubRestart => {
            hub.request_restart()?;
            C::Ok
        }
        R::HubConfigure { settings } => C::HubSettings(hub.hub_configure(settings).await?),
        R::RepoAdd { path } => C::Repo(hub.repo_add(path).await?),
        R::RepoList => C::Repos { repos: hub.repo_list().await? },
        R::RecordsList => C::Records(hub.records_list().await?),
        R::AgentList => C::Agents { agents: hub.agent_list() },
        R::WorkspaceCreate { repo_id, name, base_ref, client_request_id } => {
            C::Workspace(hub.workspace_create(repo_id, name, base_ref, client_request_id).await?)
        }
        R::WorkspaceList { include_archived } => {
            C::Workspaces { workspaces: hub.workspace_list(include_archived).await? }
        }
        R::WorkspaceArchive { id, force } => C::Workspace(hub.workspace_archive(id, force).await?),
        R::WorkspaceRestore { id } => C::Workspace(hub.workspace_restore(id).await?),
        R::WorkspaceRename { id, title } => C::Workspace(hub.workspace_rename(id, &title).await?),
        R::WorkspaceCommentDraft(drafts::WorkspaceCommentDraft { id, draft }) => {
            C::Workspace(hub.workspace_comment_draft(id, draft).await?)
        }
        R::WorkspaceRegenerateTitle { id } => C::Workspace(hub.workspace_regenerate_title(id).await?),
        R::WorkspaceStatus { id } => C::GitStatus(hub.workspace_status(id).await?),
        R::WorkspaceDiff { id } => C::GitDiff(hub.workspace_diff(id).await?),
        R::WorkspaceFiles(files::WorkspaceFiles { id }) => C::WorktreeFiles(hub.workspace_files(id).await?),
        R::WorkspaceFolderRead(files::WorkspaceFolderRead { id, path }) => {
            C::WorktreeFiles(hub.workspace_folder_read(id, path).await?)
        }
        R::WorkspaceFileRead(files::WorkspaceFileRead { id, path }) => {
            C::FileContents(hub.workspace_file_read(id, path).await?)
        }
        R::WorkspaceFileReadAt(files::WorkspaceFileReadAt { id, commit, path }) => {
            C::FileContents(hub.workspace_file_read_at(id, commit, path).await?)
        }
        R::WorkspaceFileOpenAt(files::WorkspaceFileOpenAt { id, commit, path }) => {
            C::FileOpened(hub.workspace_file_open_at(id, commit, path).await?)
        }
        R::WorkspaceFilePages(asked) => C::FilePages(hub.workspace_file_pages(asked).await?),
        R::WorkspaceSetupLog { id } => C::SetupLog { text: hub.workspace_setup_log(id).await? },
        R::WorkspaceRerunSetup { id } => C::Workspace(hub.workspace_rerun_setup(id).await?),
        R::WorkspacePullRequest { id } => C::PullRequest { pull_request: hub.workspace_pull_request(id).await? },
        R::CommentAdd { workspace_id, path, side, line, text, client_request_id } => {
            C::Comment(hub.comment_add(workspace_id, path, side, line, text, client_request_id).await?)
        }
        R::CommentList { workspace_id } => C::Comments { comments: hub.comment_list(workspace_id).await? },
        R::CommentEdit { id, text } => C::Comment(hub.comment_edit(id, text).await?),
        R::CommentDelete { id } => {
            hub.comment_delete(id).await?;
            C::Ok
        }
        R::SessionCreate { workspace_id, agent, options, client_request_id } => {
            C::Session(hub.session_create(workspace_id, agent, options, client_request_id).await?)
        }
        R::SessionEnsure { workspace_id, agent } => C::Session(hub.session_ensure(workspace_id, agent).await?),
        R::SessionList { workspace_id } => C::Sessions { sessions: hub.session_list(workspace_id).await? },
        R::SessionGet { id } => C::Session(hub.session_get(id).await?),
        R::SessionRead { id, from_seq, limit, max_bytes, include_raw } => {
            hub.session_read(id, from_seq, limit, max_bytes, include_raw).await?
        }
        R::SessionPromptList { id } => C::Prompts { prompts: hub.session_prompt_list(id).await? },
        R::SessionCompactionList { id } => C::Compactions { compactions: hub.session_compaction_list(id).await? },
        R::SessionFind { id, subagent, query } => C::Found(hub.session_find(id, subagent, &query).await?),
        R::SessionSubscribe { id, from_seq, subscription_id } => {
            subscribe(hub, connection, id, subscription_id, Following::Log { next: from_seq.max(1) }).await?
        }
        R::SessionReadPlaces { id, subagent, from_place, limit, max_bytes, include_raw } => {
            hub.session_read_places(id, subagent, from_place, limit, max_bytes, include_raw).await?
        }
        R::SessionSubagents { id } => hub.session_subagents(id).await?,
        R::SessionSubscribePlaces { id, subagent, from_place, subscription_id } => {
            let next = from_place.max(Place::new(1));
            subscribe(hub, connection, id, subscription_id, Following::Places { subagent, next }).await?
        }
        R::FeedSubscribe { subscription_id } => {
            add_subscription(connection, subscription_id, || {
                tokio::spawn(pump_feed(hub.subscribe_feed(), subscription_id, connection.frames.clone())).abort_handle()
            })?;
            C::Ok
        }
        R::SessionUnsubscribe { subscription_id } => {
            if let Some(subscription) = connection.subscriptions.lock().remove(&subscription_id) {
                subscription.abort();
            }
            C::Ok
        }
        R::SessionPrompt { id, client_msg_id, prompt } => {
            let (seq, duplicate) = hub.session_prompt(id, client_msg_id, prompt).await?;
            C::PromptRecorded { seq, duplicate }
        }
        R::AttachmentAdd { session_id, id, name, media_type, data } => {
            let kept_for = FileFor::Session(session_id);
            C::Attachment(hub.file_add(kept_for, id, name, media_type, data, AddedBy::Client).await?)
        }
        R::AttachmentRead { session_id, id } => {
            C::AttachmentData { data: hub.file_read(FileFor::Session(session_id), id).await? }
        }
        R::AttachmentRemove { session_id, id } => {
            hub.file_remove(FileFor::Session(session_id), id).await?;
            C::Ok
        }
        R::NewWorkspaceDraftKeep(drafts::NewWorkspaceDraftKeep { draft }) => {
            C::NewWorkspaceDraft(hub.new_workspace_draft_keep(draft).await?)
        }
        R::NewWorkspaceDraftFileAdd(drafts::NewWorkspaceDraftFileAdd { id, name, media_type, data }) => {
            C::Attachment(hub.file_add(FileFor::NewWorkspace, id, name, media_type, data, AddedBy::Client).await?)
        }
        R::NewWorkspaceDraftFileRead(drafts::NewWorkspaceDraftFileRead { id }) => {
            C::AttachmentData { data: hub.file_read(FileFor::NewWorkspace, id).await? }
        }
        R::NewWorkspaceDraftFileRemove(drafts::NewWorkspaceDraftFileRemove { id }) => {
            hub.file_remove(FileFor::NewWorkspace, id).await?;
            C::Ok
        }
        R::SessionAccount { id } => C::SessionAccount(hub.session_account(id).await?),
        R::AccountConnect(_) | R::AccountDisconnect(_) => {
            return Err(ApiError::new(ErrorCode::Unsupported, "this hub doesn't hold agents' logins yet"));
        }
        R::SessionQueue(queue::SessionQueue { id, client_msg_id, prompt }) => {
            C::Session(hub.session_queue(id, client_msg_id, prompt).await?)
        }
        R::SessionChangeQueued(queue::SessionChangeQueued { id, client_msg_id, text }) => {
            C::Session(hub.session_change_queued(id, client_msg_id, text).await?)
        }
        R::SessionUnqueue(queue::SessionUnqueue { id, client_msg_id }) => {
            C::Session(hub.session_unqueue(id, client_msg_id).await?)
        }
        R::SessionSendQueued(queue::SessionSendQueued { id, client_msg_id }) => {
            C::Session(hub.session_send_queued(id, client_msg_id).await?)
        }
        R::SessionApprove { id, approval_id, decision } => {
            hub.session_approve(id, approval_id, decision).await?;
            C::Ok
        }
        R::SessionConfigure { id, options } => C::Session(hub.session_configure(id, options).await?),
        R::SessionClose { id } => C::Session(hub.session_close(id).await?),
        R::SessionReopen { id } => C::Session(hub.session_reopen(id).await?),
        R::SessionMarkRead { id, through_seq } => C::Session(hub.session_mark_read(id, through_seq).await?),
        R::SessionMarkUnread { id } => C::Session(hub.session_mark_unread(id).await?),
        R::SessionCallOffWait(waits::SessionCallOffWait { id, wait }) => {
            C::Session(hub.session_call_off_wait(id, wait).await?)
        }
        R::SessionDraft(drafts::SessionDraft { id, draft }) => C::Session(hub.session_draft(id, draft).await?),
        R::SessionAnswerDraft(drafts::SessionAnswerDraft { id, draft }) => {
            C::Session(hub.session_answer_draft(id, draft).await?)
        }
        R::SessionRename { id, title } => C::Session(hub.session_rename(id, &title).await?),
        R::SessionCancel { id } => {
            hub.session_cancel(id).await?;
            C::Ok
        }
        R::SessionCompact { id, keeping } => {
            hub.session_compact(id, keeping).await?;
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
