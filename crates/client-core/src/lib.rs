//! What every client shares: the connection to a hub. No UI and no async runtime. Calls block, so
//! they belong on a background thread, and subscriptions deliver through a channel that both
//! threads and async tasks can read. This keeps GPUI's executor out of it and leaves a path open
//! for a native mobile client.
//!
//! Range loading, anchors and eviction arrive here with the desktop app in step 1.3.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use protocol::PROTOCOL_VERSION;
use protocol::client::{ApiError, ClientFrame, ClientRequest, ClientResponse, HubFrame, HubStatus, ReplyOutcome};
use protocol::event::{ApprovalDecision, SessionEvent};
use protocol::frame;
use protocol::model::{AgentKind, GitDiff, GitStatus, Repo, Seq, Session, Workspace};
use protocol::paths::Paths;
use protocol::tail::{StreamDelta, Tail};

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("the connection to the hub was lost")]
    Disconnected,
    #[error("unexpected reply from the hub: {0}")]
    Unexpected(String),
}

pub type Result<T> = std::result::Result<T, ClientError>;

/// What a subscription delivers. `Disconnected` is last. To resume, connect again and subscribe
/// from one past the last event's `seq`.
#[derive(Debug, Clone, PartialEq)]
pub enum SubscriptionItem {
    Event(SessionEvent),
    /// Replaces any streaming tail held so far.
    Tail(Tail),
    Delta(StreamDelta),
    Disconnected,
}

/// Dropping it tells the hub to stop sending.
pub struct Subscription {
    pub id: u64,
    pub items: flume::Receiver<SubscriptionItem>,
    connection: Arc<Inner>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.connection.subscriptions.lock().unwrap().remove(&self.id);
        // Not waiting for the reply: this can run on any thread, and the reply doesn't matter.
        let _ = self.connection.send_only(ClientRequest::SessionUnsubscribe { subscription_id: self.id });
    }
}

type PendingReply = flume::Sender<std::result::Result<ClientResponse, ClientError>>;

struct Inner {
    writer: Mutex<UnixStream>,
    pending: Mutex<HashMap<u64, PendingReply>>,
    subscriptions: Mutex<HashMap<u64, flume::Sender<SubscriptionItem>>>,
    next_id: AtomicU64,
    /// Set once the reader has finished, so a later call can't wait forever.
    disconnected: Mutex<bool>,
}

impl Inner {
    fn send_only(&self, request: ClientRequest) -> Result<()> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let line = frame::encode(&ClientFrame { id, request }).map_err(|e| ClientError::Unexpected(e.to_string()))?;
        self.writer.lock().unwrap().write_all(&line)?;
        Ok(())
    }
}

/// Closes the socket when the last `Client` clone goes away. The reader thread can't be what keeps
/// this alive, or it would wait forever on a socket that only it was holding open.
struct Closer(UnixStream);

impl Drop for Closer {
    fn drop(&mut self) {
        let _ = self.0.shutdown(std::net::Shutdown::Both);
    }
}

/// Dropping the last clone disconnects, which also ends its subscriptions.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
    _closer: Arc<Closer>,
}

impl Client {
    /// Connects and checks the protocol version.
    pub fn connect(socket: &Path, client_name: &str) -> Result<Client> {
        let stream = UnixStream::connect(socket)?;
        let reader = stream.try_clone()?;
        let closer = Arc::new(Closer(stream.try_clone()?));
        let inner = Arc::new(Inner {
            writer: Mutex::new(stream),
            pending: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            disconnected: Mutex::new(false),
        });
        let for_reader = inner.clone();
        std::thread::Builder::new()
            .name("hub-client-reader".to_string())
            .spawn(move || read_frames(for_reader, reader))?;
        let client = Client { inner, _closer: closer };
        client.call(ClientRequest::Hello { protocol_version: PROTOCOL_VERSION, client: client_name.to_string() })?;
        Ok(client)
    }

    /// Connects, starting a hub first if none is running. The hub is a separate process that
    /// outlives this client, which is why closing a client doesn't stop its agents.
    pub fn connect_or_start(paths: &Paths, daemon: &Path, client_name: &str) -> Result<Client> {
        match Client::connect(&paths.socket, client_name) {
            Ok(client) => return Ok(client),
            Err(ClientError::Io(e))
                if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused) => {}
            Err(e) => return Err(e),
        }
        start_hub(paths, daemon)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match Client::connect(&paths.socket, client_name) {
                Ok(client) => return Ok(client),
                Err(ClientError::Io(_)) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(e) => return Err(e),
            }
        }
    }

    pub fn call(&self, request: ClientRequest) -> Result<ClientResponse> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let line = frame::encode(&ClientFrame { id, request }).map_err(|e| ClientError::Unexpected(e.to_string()))?;
        let (reply_tx, reply_rx) = flume::bounded(1);
        self.inner.pending.lock().unwrap().insert(id, reply_tx);
        if *self.inner.disconnected.lock().unwrap() {
            self.inner.pending.lock().unwrap().remove(&id);
            return Err(ClientError::Disconnected);
        }
        if let Err(e) = self.inner.writer.lock().unwrap().write_all(&line) {
            self.inner.pending.lock().unwrap().remove(&id);
            return Err(e.into());
        }
        reply_rx.recv().map_err(|_| ClientError::Disconnected)?
    }

    pub fn hub_status(&self) -> Result<HubStatus> {
        match self.call(ClientRequest::HubStatus)? {
            ClientResponse::HubStatus(status) => Ok(status),
            other => unexpected(other),
        }
    }

    pub fn hub_shutdown(&self) -> Result<()> {
        self.expect_ok(ClientRequest::HubShutdown)
    }

    pub fn repo_add(&self, path: &str) -> Result<Repo> {
        match self.call(ClientRequest::RepoAdd { path: path.to_string() })? {
            ClientResponse::Repo(repo) => Ok(repo),
            other => unexpected(other),
        }
    }

    pub fn repo_list(&self) -> Result<Vec<Repo>> {
        match self.call(ClientRequest::RepoList)? {
            ClientResponse::Repos { repos } => Ok(repos),
            other => unexpected(other),
        }
    }

    pub fn workspace_create(&self, repo_id: &str, name: Option<String>, base_ref: Option<String>) -> Result<Workspace> {
        self.expect_workspace(ClientRequest::WorkspaceCreate { repo_id: repo_id.to_string(), name, base_ref })
    }

    pub fn workspace_list(&self, include_archived: bool) -> Result<Vec<Workspace>> {
        match self.call(ClientRequest::WorkspaceList { include_archived })? {
            ClientResponse::Workspaces { workspaces } => Ok(workspaces),
            other => unexpected(other),
        }
    }

    pub fn workspace_archive(&self, id: &str, force: bool) -> Result<Workspace> {
        self.expect_workspace(ClientRequest::WorkspaceArchive { id: id.to_string(), force })
    }

    pub fn workspace_restore(&self, id: &str) -> Result<Workspace> {
        self.expect_workspace(ClientRequest::WorkspaceRestore { id: id.to_string() })
    }

    pub fn workspace_status(&self, id: &str) -> Result<GitStatus> {
        match self.call(ClientRequest::WorkspaceStatus { id: id.to_string() })? {
            ClientResponse::GitStatus(status) => Ok(status),
            other => unexpected(other),
        }
    }

    pub fn workspace_diff(&self, id: &str) -> Result<GitDiff> {
        match self.call(ClientRequest::WorkspaceDiff { id: id.to_string() })? {
            ClientResponse::GitDiff(diff) => Ok(diff),
            other => unexpected(other),
        }
    }

    pub fn session_create(
        &self,
        workspace_id: &str,
        agent: AgentKind,
        model: Option<String>,
        permission_mode: Option<String>,
    ) -> Result<Session> {
        let request =
            ClientRequest::SessionCreate { workspace_id: workspace_id.to_string(), agent, model, permission_mode };
        self.expect_session(request)
    }

    pub fn session_list(&self, workspace_id: &str) -> Result<Vec<Session>> {
        match self.call(ClientRequest::SessionList { workspace_id: workspace_id.to_string() })? {
            ClientResponse::Sessions { sessions } => Ok(sessions),
            other => unexpected(other),
        }
    }

    pub fn session_get(&self, id: &str) -> Result<Session> {
        self.expect_session(ClientRequest::SessionGet { id: id.to_string() })
    }

    /// Returns the events and the session's newest `seq`.
    pub fn session_read(
        &self,
        id: &str,
        from_seq: Seq,
        limit: u32,
        include_raw: bool,
    ) -> Result<(Vec<SessionEvent>, Seq)> {
        let request = ClientRequest::SessionRead { id: id.to_string(), from_seq, limit, max_bytes: None, include_raw };
        match self.call(request)? {
            ClientResponse::Events { events, last_seq } => Ok((events, last_seq)),
            other => unexpected(other),
        }
    }

    pub fn session_subscribe(&self, id: &str, from_seq: Seq) -> Result<Subscription> {
        // Registered before the request goes out, because events can arrive before the reply.
        let subscription_id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (items_tx, items) = flume::unbounded();
        self.inner.subscriptions.lock().unwrap().insert(subscription_id, items_tx);
        let reply = self.expect_ok(ClientRequest::SessionSubscribe { id: id.to_string(), from_seq, subscription_id });
        if let Err(e) = reply {
            self.inner.subscriptions.lock().unwrap().remove(&subscription_id);
            return Err(e);
        }
        Ok(Subscription { id: subscription_id, items, connection: self.inner.clone() })
    }

    /// Returns the prompt's `seq`, and whether it had already been recorded.
    pub fn session_prompt(&self, id: &str, text: &str, client_msg_id: Option<String>) -> Result<(Seq, bool)> {
        let request = ClientRequest::SessionPrompt { id: id.to_string(), text: text.to_string(), client_msg_id };
        match self.call(request)? {
            ClientResponse::PromptRecorded { seq, duplicate } => Ok((seq, duplicate)),
            other => unexpected(other),
        }
    }

    pub fn session_approve(&self, id: &str, approval_id: &str, decision: ApprovalDecision) -> Result<()> {
        self.expect_ok(ClientRequest::SessionApprove {
            id: id.to_string(),
            approval_id: approval_id.to_string(),
            decision,
        })
    }

    pub fn session_cancel(&self, id: &str) -> Result<()> {
        self.expect_ok(ClientRequest::SessionCancel { id: id.to_string() })
    }

    fn expect_ok(&self, request: ClientRequest) -> Result<()> {
        match self.call(request)? {
            ClientResponse::Ok => Ok(()),
            other => unexpected(other),
        }
    }

    fn expect_workspace(&self, request: ClientRequest) -> Result<Workspace> {
        match self.call(request)? {
            ClientResponse::Workspace(workspace) => Ok(workspace),
            other => unexpected(other),
        }
    }

    fn expect_session(&self, request: ClientRequest) -> Result<Session> {
        match self.call(request)? {
            ClientResponse::Session(session) => Ok(session),
            other => unexpected(other),
        }
    }
}

fn unexpected<T>(response: ClientResponse) -> Result<T> {
    Err(ClientError::Unexpected(format!("{response:?}")))
}

fn read_frames(inner: Arc<Inner>, stream: UnixStream) {
    let mut reader = BufReader::new(stream);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let Ok(parsed) = frame::decode::<HubFrame>(&line) else {
            continue;
        };
        match parsed {
            HubFrame::Reply { id, outcome } => {
                if let Some(reply) = inner.pending.lock().unwrap().remove(&id) {
                    let _ = reply.send(match outcome {
                        ReplyOutcome::Response(response) => Ok(response),
                        ReplyOutcome::Error(error) => Err(ClientError::Api(error)),
                    });
                }
            }
            HubFrame::Event { subscription_id, event } => {
                deliver(&inner, subscription_id, SubscriptionItem::Event(event))
            }
            HubFrame::Tail { subscription_id, tail } => deliver(&inner, subscription_id, SubscriptionItem::Tail(tail)),
            HubFrame::Delta { subscription_id, delta } => {
                deliver(&inner, subscription_id, SubscriptionItem::Delta(delta))
            }
        }
    }
    *inner.disconnected.lock().unwrap() = true;
    let pending: Vec<_> = inner.pending.lock().unwrap().drain().collect();
    for (_, reply) in pending {
        let _ = reply.send(Err(ClientError::Disconnected));
    }
    for (_, subscription) in inner.subscriptions.lock().unwrap().drain() {
        let _ = subscription.send(SubscriptionItem::Disconnected);
    }
}

fn deliver(inner: &Inner, subscription_id: u64, item: SubscriptionItem) {
    if let Some(subscription) = inner.subscriptions.lock().unwrap().get(&subscription_id) {
        let _ = subscription.send(item);
    }
}

fn start_hub(paths: &Paths, daemon: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    use std::os::unix::process::CommandExt;
    // Only sets the mode if it creates the directory, because AGENTHUB_HOME can point anywhere.
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&paths.data_dir)?;
    let log = std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(&paths.log)?;
    let mut command = Command::new(daemon);
    command.arg("hub").stdin(Stdio::null()).stdout(Stdio::from(log.try_clone()?)).stderr(Stdio::from(log));
    // SAFETY: setsid is async-signal-safe, which is all pre_exec requires. It detaches the hub from
    // this terminal and process group, so closing the client doesn't reach it.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn().map_err(|e| std::io::Error::new(e.kind(), format!("starting {}: {e}", daemon.display())))?;
    Ok(())
}
