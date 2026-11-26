//! Having the run connect again to a tool server of the hub's, so that it has it as it is: one the person has signed in to,
//! or edited.
//!
//! What's due is worked out from how things stand, each time one of them changes, rather than from the sign-in or the
//! edit happening: how the run last said each server is, and which of the hub's each is, as its worker says, how the run
//! has had each, and how the worker has each, as its link says. So one that's saved as the agent starts, or while its
//! worker's connection is down, isn't missed. A server the run says nothing of isn't one it was launched with. On a turn,
//! what's due is only said of each server, for clients, as [`Looked::mark`](super::had_servers) does, so that no client
//! works that out for itself, and it's worked out again as the turn ends, and before each prompt.

use protocol::capability::setup::ToolServerState;
use protocol::model::{RunId, ToolServerId};
use tracing::{info, warn};

use super::had_servers::{HadServers, Running};
use super::messages::Command;
use super::{AgentRun, LiveSession, LiveState};
use crate::hub::{Hub, SessionToolServers};
use crate::tool_servers::{ServerAsSent, ServersAsSent};
use crate::worker_link::WorkerLink;

impl LiveState {
    /// `run` has had the server `id` as it is `now`, as if it had been asked to connect to it again for that.
    pub(super) fn has_had(&mut self, run: RunId, id: &ToolServerId, now: ServerAsSent) {
        if let Some(had) = self.had_servers.as_mut().filter(|had| had.is_of(run)) {
            had.has_had(id, now);
        }
    }
}

impl LiveSession {
    /// Has the run asked to connect again to what's due, or clients told what waits for its turn to end, if there's
    /// either to do, after whatever was asked of the session before.
    pub(crate) fn wake_reconnects(&self) {
        let Ok(hub) = self.hub() else { return };
        let Some(link) = hub.running_worker(&self.workspace_id) else { return };
        let Some((run, Some(said))) = self.said_of_its_servers(&hub, &link) else { return };
        let finds = |had: &mut HadServers, now: &_, running: &_| had.look(now, &said, running).finds_anything(&said);
        if self.of_the_run(&link, run, finds) == Some(true) {
            self.ask_for_reconnects();
        }
    }

    /// The same, whether or not anything is.
    pub(super) fn ask_for_reconnects(&self) {
        if let Err(e) = self.commands.try_send(Command::ReconnectToolServers) {
            warn!(
                name: "session.reconnects_unasked",
                session_id = %self.id,
                error = %e,
                "Couldn't ask for an agent's tool servers to be connected to again"
            );
        }
    }

    /// Asks the run to connect again to each server due, on the link whose worker has it as it is. What comes of it is
    /// what the run says of its servers, and what asking comes to is only logged: a run that has gone has none, and the
    /// next is given the server as it is, and one whose ask doesn't reach the agent is asked again the next time, as
    /// [`HadServers::take`] says. Clients are told what still waits.
    pub(super) async fn reconnect_tool_servers(&self) {
        let Ok(hub) = self.hub() else { return };
        let Some(link) = hub.running_worker(&self.workspace_id) else { return };
        for (run, id, server) in self.take_due(&hub, &link) {
            info!(
                name: "session.tool_server_reconnecting",
                session_id = %self.id,
                server_id = %id,
                "Asking an agent to connect to a tool server that's been signed in to or edited"
            );
            if let Err(e) = self.connect_again(&link, run, &server).await {
                // Not its message, which can be the CLI's own, and quote the person's settings.
                warn!(
                    name: "session.tool_server_not_reconnected",
                    session_id = %self.id,
                    server_id = %id,
                    code = ?e.code,
                    "Couldn't have an agent connect to a tool server again"
                );
            }
        }
        self.mark_what_waits().await;
    }

    /// The servers the run is to be asked to connect to again, and the run, as [`HadServers::take`] says.
    fn take_due(&self, hub: &Hub, link: &WorkerLink) -> Vec<(RunId, ToolServerId, ToolServerState)> {
        let Some((run, Some(mut said))) = self.said_of_its_servers(hub, link) else { return Vec::new() };
        if let Some(reloading) = &self.state.lock().reloading {
            reloading.starting(run, &mut said);
        }
        let due = self.of_the_run(link, run, |had, now, running| {
            let looked = had.look(now, &said, running);
            had.take(looked)
        });
        due.into_iter().flatten().map(|(id, server)| (run, id, server)).collect()
    }

    /// Keeps when the run is to connect again to each server it hasn't had as it is with what it last said of them.
    pub(super) async fn mark_what_waits(&self) {
        let Ok(hub) = self.hub() else { return };
        let marked = hub.observation_worked_out_again::<SessionToolServers>(self.observed_as(), |said| {
            let run = self.state.lock().agent.map(|agent| agent.run);
            if let Some(run) = run {
                self.mark_waiting(run, said);
            }
        });
        marked.await;
    }

    /// Marks those of `said`, which `run` says it has, with when it's to connect to each again, and no others: none where
    /// nothing tells, as while its worker's connection is down.
    pub(super) fn mark_waiting(&self, run: RunId, said: &mut [ToolServerState]) {
        let Ok(hub) = self.hub() else { return };
        let link = hub.running_worker(&self.workspace_id);
        let looked =
            link.and_then(|link| self.of_the_run(&link, run, |had, now, running| had.look(now, said, running)));
        looked.unwrap_or_default().mark(said);
    }

    /// What `read` makes of how `run` has had its servers, how they are on `link`, and the run itself, if it's the one
    /// going, on that link.
    fn of_the_run<T>(
        &self,
        link: &WorkerLink,
        run: RunId,
        read: impl FnOnce(&mut HadServers, &ServersAsSent, &Running) -> T,
    ) -> Option<T> {
        let now = link.tool_servers().borrow().clone();
        let mut state = self.state.lock();
        state.agent.filter(|agent| *agent == AgentRun { link: link.id, run })?;
        let running = Running { claims: state.run_claims()?.clone(), on_a_turn: state.on_a_turn() };
        let had = state.had_servers.as_mut().filter(|had| had.is_of(run))?;
        Some(read(had, &now, &running))
    }
}
