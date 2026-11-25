use super::path::path_of;
use super::{Failure, Upstream};
use hyper::Uri;
use hyper::header::{HeaderName, HeaderValue};
use parking_lot::Mutex;
use protocol::capability::setup::ServerName;
use protocol::model::{ToolServerId, ToolServerName};
use protocol::worker::{ProxiedServer, ServerHeader};
use std::sync::Arc;

/// The hub's tool servers, as the hub last set them, and the client that reaches them.
#[derive(Default)]
pub(crate) struct Proxy {
    servers: Mutex<Vec<Arc<Server>>>,
    pub(super) upstream: Upstream,
}

impl Proxy {
    /// Takes the place of the servers before. A request under way goes on with the server it began with.
    pub(crate) fn set(&self, servers: Vec<ProxiedServer>) {
        let servers = servers.into_iter().map(|server| Arc::new(Server::new(server))).collect();
        *self.servers.lock() = servers;
    }

    /// Each server's name, as agents are given it, and its path on the worker.
    pub(in super::super) fn paths(&self) -> Vec<(ServerName, String)> {
        self.servers.lock().iter().map(|server| (server.name.clone(), path_of(&server.id))).collect()
    }

    pub(super) fn server(&self, id: &ToolServerId) -> Option<Arc<Server>> {
        self.servers.lock().iter().find(|server| server.id == *id).cloned()
    }
}

pub(super) struct Server {
    pub(super) id: ToolServerId,
    name: ServerName,
    pub(super) label: ToolServerName,
    pub(super) target: Result<Target, Failure>,
}

impl Server {
    fn new(server: ProxiedServer) -> Self {
        let ProxiedServer { id, name, label, url, header } = server;
        let target = Target::new(url.as_str(), header).ok_or(Failure::Unusable);
        Self { id, name, label, target }
    }
}

/// Where a server is, and the header that says who's asking.
pub(super) struct Target {
    pub(super) uri: Uri,
    pub(super) header: Option<(HeaderName, HeaderValue)>,
}

impl Target {
    /// `None` for a URL that isn't HTTP's, or a header HTTP can't carry.
    fn new(url: &str, header: Option<ServerHeader>) -> Option<Self> {
        let uri: Uri = url.parse().ok()?;
        uri.host()?;
        if !matches!(uri.scheme_str(), Some("http" | "https")) {
            return None;
        }
        let header = match header {
            Some(header) => Some(sent(&header)?),
            None => None,
        };
        Some(Self { uri, header })
    }
}

fn sent(header: &ServerHeader) -> Option<(HeaderName, HeaderValue)> {
    let name = HeaderName::from_bytes(header.name.as_str().as_bytes()).ok()?;
    let mut value = HeaderValue::from_str(header.value.as_str()).ok()?;
    value.set_sensitive(true);
    Some((name, value))
}
