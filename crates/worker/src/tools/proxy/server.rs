use super::path::path_of;
use super::{Connections, Failure, Sent, UnderWay, Upstream};
use hyper::Uri;
use hyper::header::{HeaderName, HeaderValue};
use parking_lot::Mutex;
use protocol::capability::setup::{ServerName, ToolServerState};
use protocol::model::{RunId, SessionId, ToolServerId, ToolServerName, ToolServerPlace};
use protocol::worker::{ProxiedServer, ServerHeader, ServerSignIn, TokenNumber};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::watch;

/// The hub's tool servers, as the hub last set them, where each agent is connected to each, and the client that reaches
/// them.
#[derive(Default)]
pub(crate) struct Proxy {
    servers: Mutex<Vec<Arc<Server>>>,
    /// Where each is, for a stream of one's to end as it's moved or removed.
    places: watch::Sender<HashMap<ToolServerId, ToolServerPlace>>,
    pub(super) connections: Connections,
    /// The calls under way to each, kept here rather than on each server, which each set makes again.
    pub(super) under_way: UnderWay,
    pub(super) upstream: Upstream,
}

impl Proxy {
    /// Takes the place of the servers before. A request under way goes on with the server it began with.
    pub(crate) fn set(&self, servers: Vec<ProxiedServer>) {
        let servers: Vec<_> = servers.into_iter().map(|server| Arc::new(Server::new(server))).collect();
        let places = servers.iter().map(|server| (server.id.clone(), server.place.clone())).collect();
        *self.servers.lock() = servers;
        self.places.send_replace(places);
    }

    /// The agent of `session_id` starts, as its `run`, so nothing is known of where it's connected.
    pub(in super::super) fn starts(&self, session_id: &SessionId, run: RunId) {
        self.connections.starts(session_id, run);
    }

    /// The agent of `session_id` that started as `run` has exited, so what's known of where it was connected goes.
    pub(crate) fn ended(&self, session_id: &SessionId, run: RunId) {
        self.connections.ended(session_id, run);
    }

    /// `said`, what the agent of `session_id` says of its servers, with where it last connected to each of the hub's.
    pub(crate) fn saying_where(&self, session_id: &SessionId, mut said: Vec<ToolServerState>) -> Vec<ToolServerState> {
        for server in &mut said {
            server.connected_to = server.hubs.as_ref().and_then(|id| self.connections.place(session_id, id));
        }
        said
    }

    /// Until `server` is at another place than it is, or isn't one of the hub's.
    pub(super) fn gone(&self, server: &Server) -> impl Future<Output = ()> + Send + 'static {
        let (mut places, id, place) = (self.places.subscribe(), server.id.clone(), server.place.clone());
        async move {
            let gone = |places: &HashMap<_, _>| places.get(&id) != Some(&place);
            // An error says the worker has stopped serving.
            let _gone = places.wait_for(gone).await;
        }
    }

    /// Each server, its name, as agents are given it, and its path on the worker.
    pub(in super::super) fn paths(&self) -> Vec<(ToolServerId, ServerName, String)> {
        let path = |server: &Arc<Server>| (server.id.clone(), server.name.clone(), path_of(&server.id));
        self.servers.lock().iter().map(path).collect()
    }

    pub(super) fn server(&self, id: &ToolServerId) -> Option<Arc<Server>> {
        self.servers.lock().iter().find(|server| server.id == *id).cloned()
    }
}

pub(super) struct Server {
    pub(super) id: ToolServerId,
    name: ServerName,
    pub(super) label: ToolServerName,
    pub(super) place: ToolServerPlace,
    pub(super) target: Result<Target, Failure>,
}

impl Server {
    pub(super) fn new(server: ProxiedServer) -> Self {
        let ProxiedServer { id, name, label, url, header, sign_in } = server;
        let place = url.place();
        let target = match sign_in {
            Some(ServerSignIn::NeedsSignIn) => Err(Failure::NeedsSignIn),
            sign_in => Target::new(url.as_str(), header, sign_in).ok_or(Failure::Unusable),
        };
        Self { id, name, label, place, target }
    }
}

/// Where a server is, and the header that says who's asking.
pub(super) struct Target {
    pub(super) uri: Uri,
    pub(super) header: Option<(HeaderName, HeaderValue)>,
    sign_in: Option<ServerSignIn>,
}

impl Target {
    /// Which access token a request to it carries, if it's one the hub could have another of.
    pub(super) fn token(&self) -> Option<TokenNumber> {
        match self.sign_in? {
            ServerSignIn::Works { token } => Some(token),
            ServerSignIn::NeedsSignIn | ServerSignIn::Unknown => None,
        }
    }

    pub(super) fn sent(&self) -> Sent {
        match (&self.sign_in, &self.header) {
            (Some(_), _) => Sent::AccessToken,
            (None, Some(_)) => Sent::Key,
            (None, None) => Sent::Nothing,
        }
    }

    /// `None` for a URL that isn't HTTP's, or a header HTTP can't carry.
    fn new(url: &str, header: Option<ServerHeader>, sign_in: Option<ServerSignIn>) -> Option<Self> {
        let uri: Uri = url.parse().ok()?;
        uri.host()?;
        if !matches!(uri.scheme_str(), Some("http" | "https")) {
            return None;
        }
        let header = match header {
            Some(header) => Some(sent(&header)?),
            None => None,
        };
        Some(Self { uri, header, sign_in })
    }
}

fn sent(header: &ServerHeader) -> Option<(HeaderName, HeaderValue)> {
    let name = HeaderName::from_bytes(header.name.as_str().as_bytes()).ok()?;
    let mut value = HeaderValue::from_str(header.value.as_str()).ok()?;
    value.set_sensitive(true);
    Some((name, value))
}
