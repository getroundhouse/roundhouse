//! Having the run connect again to one of its tool servers, whoever asks: the person, for one that can't be reached or
//! whose tools changed, and the hub, once the person has signed in to one, as [`super::reconnecting`] says.

use std::collections::BTreeSet;

use protocol::capability::Control;
use protocol::capability::setup::{ServerName, ServerState, ToolServerState};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::RunId;
use protocol::worker::Delivery;

use super::{AgentRun, LiveSession};
use crate::hub::{Hub, SessionToolServers};
use crate::worker_link::WorkerLink;

/// The servers a run has been asked to connect to again, and hasn't said anything of its servers since. It says each is
/// starting as it's asked, but that comes after its worker's answer, and until it has come, what it last said doesn't show
/// that it's been asked.
pub(super) struct Reloading {
    run: RunId,
    asked: BTreeSet<ServerName>,
}

impl Reloading {
    pub(super) fn of(run: RunId) -> Self {
        Self { run, asked: BTreeSet::new() }
    }

    /// `said`, what `run` last said of its servers, with those it has been asked to connect to since starting, as it's
    /// about to say they are.
    pub(super) fn starting(&self, run: RunId, said: &mut [ToolServerState]) {
        for server in said.iter_mut().filter(|said| self.run == run && self.asked.contains(&said.name)) {
            server.state = ServerState::Starting;
        }
    }
}

impl LiveSession {
    /// Has the run going connect again to the server it has as `server`, after whatever was asked of it before. Refused as
    /// [`SessionReloadToolServer`](protocol::client::agent::SessionReloadToolServer) says. No run is started for it.
    pub(super) async fn reload_tool_server(&self, server: &ServerName) -> Result<(), ApiError> {
        let hub = self.hub()?;
        let link = hub.running_worker(&self.workspace_id).ok_or_else(not_running)?;
        let (run, said) = self.said_of_its_servers(&hub, &link).ok_or_else(not_running)?;
        let server = said
            .iter()
            .flatten()
            .find(|said| said.name == *server)
            .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "the agent hasn't said it has that MCP server"))?;
        self.connect_again(&link, run, server).await
    }

    /// Asks `run`, on `link`, to connect again to `server`, which it last said is as this says, unless it has been asked to
    /// and hasn't said anything of its servers since. It counts as having had the server by what it's signed in to by now,
    /// so it isn't asked again for that. What comes of it is what the run says of its servers.
    pub(super) async fn connect_again(
        &self,
        link: &WorkerLink,
        run: RunId,
        server: &ToolServerState,
    ) -> Result<(), ApiError> {
        let name = server.name.clone();
        let control = match server.state {
            ServerState::Connected => Control::Reload { server: name.clone() },
            ServerState::Failed | ServerState::NeedsSignIn => Control::Reconnect { server: name.clone() },
            ServerState::Starting => return Err(conflict("the agent is connecting to that MCP server already")),
            ServerState::Disabled => return Err(conflict("that MCP server is turned off in the CLI's own settings")),
            ServerState::Unknown => return Err(conflict("the agent hasn't said how that MCP server is")),
        };
        let claims = self.run_claims().ok_or_else(not_running)?;
        if !control.allowed(&claims) {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "the agent can't connect to that MCP server again"));
        }
        let signed_in_by = server.hubs.as_ref().and_then(|id| link.signed_in().borrow().by(id).cloned());
        {
            let mut state = self.state.lock();
            if state.agent != Some(AgentRun { link: link.id, run }) {
                return Err(not_running());
            }
            if let (Some(id), Some(by)) = (&server.hubs, signed_in_by) {
                state.has_had(run, id, by);
            }
            let reloading = state.reloading.get_or_insert_with(|| Reloading::of(run));
            if reloading.run != run {
                *reloading = Reloading::of(run);
            }
            if !reloading.asked.insert(name.clone()) {
                return Ok(());
            }
        }
        let asked = self.control_claimed(link, control, &claims).await.map_err(|e| {
            // Only a hub that doesn't know a client's request says so to it.
            if e.code == ErrorCode::Unsupported { ApiError::new(ErrorCode::Worker, e.message) } else { e }
        });
        let refused = match asked {
            Ok(Delivery::Delivered) => return Ok(()),
            Ok(Delivery::AgentNotRunning) => not_running(),
            Ok(Delivery::AfterTheTurn) => conflict("the agent won't connect to it again while its turn runs"),
            Err(e) => e,
        };
        if let Some(reloading) = self.state.lock().reloading.as_mut().filter(|reloading| reloading.run == run) {
            reloading.asked.remove(&name);
        }
        Err(refused)
    }

    /// Takes in that `run` has said how its servers stand, which says from then on what it was asked of them before.
    pub(super) fn said_its_servers(&self, run: RunId) {
        self.state.lock().reloading.take_if(|reloading| reloading.run == run);
    }

    /// The run going, if it's on `link`, and the servers it has last said it has, if it has said.
    pub(super) fn said_of_its_servers(
        &self,
        hub: &Hub,
        link: &WorkerLink,
    ) -> Option<(RunId, Option<Vec<ToolServerState>>)> {
        let agent = self.state.lock().agent.filter(|agent| agent.link == link.id)?;
        // Forgotten as another run starts, so it's this one's, while this one is current after too.
        let said = hub.observation_value::<SessionToolServers>(&self.observed_as());
        self.state.lock().agent.filter(|now| *now == agent)?;
        Some((agent.run, said))
    }
}

fn not_running() -> ApiError {
    conflict("the chat's agent isn't running")
}

fn conflict(why: &str) -> ApiError {
    ApiError::new(ErrorCode::Conflict, why)
}
