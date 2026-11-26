use super::http;
use super::proxy::Proxy;
use crate::asks::Asks;
use agents::HubToolServer;
use parking_lot::Mutex;
use protocol::Build;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{RunId, SessionId};
use protocol::worker::ToolsToken;
use std::collections::HashMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio::task::{AbortHandle, JoinSet};
use tracing::{info, warn};

/// The path the server answers on, as MCP's own examples have it.
pub(super) const PATH: &str = "/mcp";

/// Something listening on 127.0.0.1 answers at once, and nothing listening refuses at once, so this is only reached
/// by something stuck, which isn't worth taking the port over.
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);

/// Long enough for a connection that failed to be taken, such as for want of file descriptors, to free what it needs.
const ACCEPT_PAUSE: Duration = Duration::from_millis(100);

/// More than a workspace's agents keep open. Each keeps one or two to the worker's own tools, and to each of the hub's
/// servers one for a call under way and one for its stream of events, if it has one, so four agents with ten servers
/// keep 88. More would use up descriptors that the agents, git and scripts need too: a connection to one of the hub's
/// servers takes a second for the worker's own to it, so this many take 192, of the 256 a Mac gives a process it starts
/// that doesn't ask for more. A server that takes calls and never answers them holds only a few of these, since the proxy
/// sends each server only so many calls at once.
pub(super) const MAX_CONNECTIONS: usize = 96;

/// Whose a call is: the agent in a session, which works in a worktree, where what it writes points to images.
#[derive(Debug)]
pub(crate) struct Caller {
    pub(crate) session_id: SessionId,
    pub(crate) worktree: PathBuf,
}

/// Where an agent reaches what the worker serves.
#[derive(Debug)]
pub(crate) struct Addresses {
    /// The worker's own tools.
    pub(crate) tools: String,
    /// Each of the hub's tool servers, as an agent is given it.
    pub(crate) servers: Vec<HubToolServer>,
}

pub(crate) struct Tools {
    pub(super) asks: Arc<Asks>,
    /// The worker's, which the server says it is.
    pub(super) build: Build,
    /// How often the agent's CLI is to ask after a wait, as [`crate::Timers::wait_poll`] says.
    pub(super) wait_poll: Duration,
    /// Whose each token is, as each agent starts. Kept while the worker runs, since a session's token doesn't change.
    pub(super) tokens: Mutex<HashMap<ToolsToken, Arc<Caller>>>,
    /// The hub's tool servers, which calls are passed on to.
    pub(crate) proxy: Proxy,
    /// Held while it starts serving, so two agents starting at once can't start two servers, and one can't start once
    /// the worker is stopping.
    pub(super) serving: tokio::sync::Mutex<Serving>,
}

pub(super) enum Serving {
    NotYet,
    On {
        port: u16,
        accepting: AbortHandle,
    },
    /// The worker is stopping, so nothing may serve again, or it would keep the worker running.
    Stopped,
}

impl Tools {
    pub(crate) fn new(asks: Arc<Asks>, build: Build, wait_poll: Duration) -> Self {
        let (tokens, serving) = (Mutex::new(HashMap::new()), tokio::sync::Mutex::new(Serving::NotYet));
        Self { asks, build, wait_poll, tokens, proxy: Proxy::default(), serving }
    }

    /// Lets calls carrying `token` through, as `caller`'s, whose agent starts as `run`.
    pub(crate) fn admit(&self, token: ToolsToken, caller: Caller, run: RunId) {
        self.proxy.starts(&caller.session_id, run);
        self.tokens.lock().insert(token, Arc::new(caller));
    }

    /// Where they're served, and the hub's tool servers as it last set them, once they are.
    pub(crate) async fn addresses(&self) -> Option<Addresses> {
        let port = match *self.serving.lock().await {
            Serving::On { port, .. } => port,
            Serving::NotYet | Serving::Stopped => return None,
        };
        let at = |path: &str| format!("http://{}:{port}{path}", Ipv4Addr::LOCALHOST);
        let servers = self.proxy.paths().into_iter();
        let servers = servers.map(|(id, name, path)| HubToolServer { id, name, url: at(&path) }).collect();
        Some(Addresses { tools: at(PATH), servers })
    }

    pub(super) fn caller_for(&self, token: &ToolsToken) -> Option<Arc<Caller>> {
        self.tokens.lock().get(token).cloned()
    }

    /// Serves on `port` if nothing else is on it, and otherwise on a port the system chooses, and says which. Once
    /// serving, says the port it's on, and once stopped, refuses.
    pub(crate) async fn serve(self: &Arc<Self>, port: Option<u16>) -> Result<u16, ApiError> {
        let mut serving = self.serving.lock().await;
        match *serving {
            Serving::On { port, .. } => return Ok(port),
            Serving::Stopped => return Err(ApiError::new(ErrorCode::Worker, "the worker is stopping")),
            Serving::NotYet => {}
        }
        let bound = match port {
            Some(port) => match listen_again(port).await {
                Ok(listener) => Ok(listener),
                Err(e) => {
                    info!(
                        name: "tools.port_moved",
                        port,
                        error = %e,
                        "The worker's tools go on another port, since they can't have theirs again"
                    );
                    listen(0, false)
                }
            },
            None => listen(0, false),
        };
        let couldnt =
            |e: io::Error| ApiError::new(ErrorCode::Worker, format!("couldn't serve the worker's tools: {e}"));
        let listener = bound.map_err(couldnt)?;
        let port = listener.local_addr().map_err(couldnt)?.port();
        let accepting = tokio::spawn(accept(listener, self.clone(), port)).abort_handle();
        *serving = Serving::On { port, accepting };
        Ok(port)
    }

    /// Stops serving, along with every connection, as the worker stops, for good.
    pub(crate) async fn stop(&self) {
        if let Serving::On { accepting, .. } = std::mem::replace(&mut *self.serving.lock().await, Serving::Stopped) {
            accepting.abort();
        }
    }
}

/// Listens on the port the tools were on before, as a worker the hub started again does, so its agents find them
/// where they were. Its connections can still be closing, which refuses the port to a listener that doesn't say it
/// may reuse it. But one that says so can also listen over another program listening on every address, on a Mac, and
/// take what's sent to it on 127.0.0.1. So there it only says so when nothing answers there.
async fn listen_again(port: u16) -> io::Result<TcpListener> {
    match listen(port, false) {
        Err(e) if e.kind() == io::ErrorKind::AddrInUse && cfg!(unix) && !answered(port).await => listen(port, true),
        listened => listened,
    }
}

/// A listener on 127.0.0.1, which says it may reuse its port if `reuse` does, and always on Linux. Linux lets a
/// listener have a port whose connections are still closing only if the listener before it said so too, and never
/// lets one listen over another, so saying so there takes nothing from anyone. Windows lets a listener have such a
/// port anyway, and saying so there would take the port from whoever has it, so `reuse` is only for Unix.
pub(super) fn listen(port: u16, reuse: bool) -> io::Result<TcpListener> {
    let socket = TcpSocket::new_v4()?;
    if cfg!(target_os = "linux") || reuse {
        socket.set_reuseaddr(true)?;
    }
    socket.bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
    socket.listen(128)
}

/// Whether something is listening on the port, on 127.0.0.1.
pub(super) async fn answered(port: u16) -> bool {
    let connecting = TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port)));
    !matches!(tokio::time::timeout(PROBE_TIMEOUT, connecting).await, Ok(Err(_)))
}

#[expect(
    clippy::disallowed_methods,
    reason = "the pause after a connection that couldn't be taken, for what it needed to come free"
)]
async fn accept(listener: TcpListener, tools: Arc<Tools>, port: u16) {
    // Dropped when serving stops, which ends every connection with it.
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) if connections.len() >= MAX_CONNECTIONS => {
                    warn!(
                        name: "tools.connection_refused",
                        open = MAX_CONNECTIONS,
                        "The worker's tools turned a connection away, with as many open as they take"
                    );
                    drop(stream);
                }
                Ok((stream, _)) => {
                    connections.spawn(http::serve_connection(stream, tools.clone(), port));
                }
                Err(e) => {
                    // Such as running out of file descriptors, which a moment can free.
                    warn!(name: "tools.connection_failed", error = %e, "The worker's tools couldn't take a connection");
                    tokio::time::sleep(ACCEPT_PAUSE).await;
                }
            },
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
}
