//! The worker's tools, which it serves to its agents over MCP, on 127.0.0.1, and answers by asking its hub. Each agent
//! is given a token by its hub, which says whose a call is, so the server is for this worker's agents alone.

use std::collections::HashMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::SessionId;
use protocol::worker::ToolsToken;
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio::task::{AbortHandle, JoinSet};
use tracing::{info, warn};

use crate::asks::Asks;

mod http;
mod mcp;
mod pull_request;
#[cfg(test)]
mod tests;

/// The path the server answers on, as MCP's own examples have it.
const PATH: &str = "/mcp";

/// Something listening on 127.0.0.1 answers at once, and nothing listening refuses at once, so this is only reached
/// by something stuck, which isn't worth taking the port over.
const PROBE_TIMEOUT: Duration = Duration::from_secs(1);

/// Long enough for a connection that failed to be taken, such as for want of file descriptors, to free what it needs.
const ACCEPT_PAUSE: Duration = Duration::from_millis(100);

/// Far more than a workspace's agents keep open, each of which keeps one or two. More would only use up descriptors
/// that the agents, git and scripts need too.
const MAX_CONNECTIONS: usize = 64;

pub(crate) struct Tools {
    asks: Arc<Asks>,
    /// Whose each token is, as each agent starts. Kept while the worker runs, since a session's token doesn't change.
    tokens: Mutex<HashMap<ToolsToken, SessionId>>,
    /// Held while it starts serving, so two agents starting at once can't start two servers, and one can't start once
    /// the worker is stopping.
    serving: tokio::sync::Mutex<Serving>,
}

enum Serving {
    NotYet,
    On {
        port: u16,
        accepting: AbortHandle,
    },
    /// The worker is stopping, so nothing may serve again, or it would keep the worker running.
    Stopped,
}

impl Tools {
    pub(crate) fn new(asks: Arc<Asks>) -> Self {
        Self { asks, tokens: Mutex::new(HashMap::new()), serving: tokio::sync::Mutex::new(Serving::NotYet) }
    }

    /// Lets calls carrying `token` through, as the agent in `session_id`'s.
    pub(crate) fn admit(&self, token: ToolsToken, session_id: SessionId) {
        self.tokens.lock().insert(token, session_id);
    }

    /// Where they're served, once they are.
    pub(crate) async fn url(&self) -> Option<String> {
        match *self.serving.lock().await {
            Serving::On { port, .. } => Some(format!("http://{}:{port}{PATH}", Ipv4Addr::LOCALHOST)),
            Serving::NotYet | Serving::Stopped => None,
        }
    }

    fn session_for(&self, token: &ToolsToken) -> Option<SessionId> {
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
                    info!(port, "the worker's tools go on another port, since they can't have theirs again: {e}");
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
fn listen(port: u16, reuse: bool) -> io::Result<TcpListener> {
    let socket = TcpSocket::new_v4()?;
    if cfg!(target_os = "linux") || reuse {
        socket.set_reuseaddr(true)?;
    }
    socket.bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
    socket.listen(128)
}

/// Whether something is listening on the port, on 127.0.0.1.
async fn answered(port: u16) -> bool {
    let connecting = TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port)));
    !matches!(tokio::time::timeout(PROBE_TIMEOUT, connecting).await, Ok(Err(_)))
}

async fn accept(listener: TcpListener, tools: Arc<Tools>, port: u16) {
    // Dropped when serving stops, which ends every connection with it.
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) if connections.len() >= MAX_CONNECTIONS => {
                    warn!("the worker's tools turned a connection away, with {MAX_CONNECTIONS} open already");
                    drop(stream);
                }
                Ok((stream, _)) => {
                    connections.spawn(http::serve_connection(stream, tools.clone(), port));
                }
                Err(e) => {
                    // Such as running out of file descriptors, which a moment can free.
                    warn!("the worker's tools couldn't take a connection: {e}");
                    tokio::time::sleep(ACCEPT_PAUSE).await;
                }
            },
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
}
