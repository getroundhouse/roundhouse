use super::path::path_of;
use super::{Connections, Failure, Sent, UnderWay, Upstream};
use protocol::capability::setup::{ServerName, ToolServerState};
use protocol::model::{RunId, SessionId, ToolServerId, ToolServerName, ToolServerPlace};
use protocol::worker::{ProxiedServer, ServerSignIn, TokenNumber};
use std::sync::Arc;
use tokio::sync::watch;

/// The hub's tool servers, as the hub last set them, where each agent is connected to each, and the client that reaches
/// them.
#[derive(Default)]
pub(crate) struct Proxy {
    /// Watched for a stream of one's to end as it's moved or removed.
    servers: watch::Sender<Vec<Arc<Server>>>,
    pub(super) connections: Connections,
    /// The calls under way to each, kept here rather than on each server, which each set makes again.
    pub(super) under_way: UnderWay,
    pub(super) upstream: Upstream,
}

impl Proxy {
    /// Takes the place of the servers before. A request under way goes on with the server it began with.
    pub(crate) fn set(&self, servers: Vec<ProxiedServer>) {
        self.servers.send_replace(servers.into_iter().map(|server| Arc::new(Server::new(server))).collect());
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
        let (mut servers, id, place) = (self.servers.subscribe(), server.id.clone(), server.place.clone());
        async move {
            let gone =
                |servers: &Vec<Arc<Server>>| !servers.iter().any(|server| server.id == id && server.place == place);
            // An error says the worker has stopped serving.
            let _gone = servers.wait_for(gone).await;
        }
    }

    /// Each server, its name, as agents are given it, and its path on the worker.
    pub(in super::super) fn paths(&self) -> Vec<(ToolServerId, ServerName, String)> {
        let path = |server: &Arc<Server>| (server.id.clone(), server.name.clone(), path_of(&server.id));
        self.servers.borrow().iter().map(path).collect()
    }

    pub(super) fn server(&self, id: &ToolServerId) -> Option<Arc<Server>> {
        self.servers.borrow().iter().find(|server| server.id == *id).cloned()
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
            sign_in => {
                let server = tool_server_http::Server::new(&url, header.as_ref());
                server.map(|server| Target { server, sign_in }).ok_or(Failure::Unusable)
            }
        };
        Self { id, name, label, place, target }
    }
}

/// Where a server is, the header that says who's asking, and whether that's an access token of a sign-in.
pub(super) struct Target {
    pub(super) server: tool_server_http::Server,
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
        match (&self.sign_in, self.server.keyed()) {
            (Some(_), _) => Sent::AccessToken,
            (None, true) => Sent::Key,
            (None, false) => Sent::Nothing,
        }
    }
}
