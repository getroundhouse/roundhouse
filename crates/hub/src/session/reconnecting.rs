//! Having the run connect again to a tool server of the hub's, once the person has signed in to it.
//!
//! What's due is worked out from how things stand, each time one of them changes, rather than from the sign-in
//! happening: how the run last said each server is, and which of the hub's each is, as its worker says, and which of
//! them the worker has the token of, as its link says. So a sign-in that finishes as the agent starts, or while its
//! worker's connection is down, isn't missed. A server the run says nothing of isn't one it was launched with.

use protocol::capability::Control;
use protocol::capability::setup::{ServerState, ToolServerState};
use protocol::model::{RunId, ToolServerId, ToolSignInId};
use tracing::{info, warn};

use super::messages::Command;
use super::{LiveSession, LiveState};
use crate::hub::Hub;
use crate::tool_servers::SignedInBy;
use crate::worker_link::WorkerLink;

/// The sign-in a run has had each of its tool servers by: those signed in to as it launched, and each it has been asked to
/// connect to again by, whoever asked, or was connected to already. None for a run carried on with from an earlier hub,
/// since nothing says what it was asked, so one that failed is asked once more. *Unconfirmed.*
pub(super) struct HadSignIns {
    run: RunId,
    had: SignedInBy,
}

/// A server whose sign-in is one the run hasn't had it by.
struct Newly {
    id: ToolServerId,
    by: ToolSignInId,
    server: ToolServerState,
    /// Whether the run isn't connected to it.
    asks: bool,
}

impl HadSignIns {
    pub(super) fn new(run: RunId, had: SignedInBy) -> Self {
        Self { run, had }
    }

    /// Those of `said`, the servers the run says it has, that it hasn't had by what they're `signed_in` to by. One it
    /// hasn't said, or that's starting, waits until it says how that came out, and one turned off in the CLI's own
    /// settings is the person's to turn on.
    fn newly(&self, signed_in: &SignedInBy, said: &[ToolServerState]) -> impl Iterator<Item = Newly> {
        said.iter().filter_map(move |server| {
            let id = server.hubs.as_ref()?;
            let by = signed_in.by(id).filter(|by| self.had.by(id) != Some(by))?;
            let asks = match server.state {
                ServerState::Failed | ServerState::NeedsSignIn => true,
                ServerState::Connected => false,
                ServerState::Starting | ServerState::Disabled | ServerState::Unknown => return None,
            };
            Some(Newly { id: id.clone(), by: by.clone(), server: server.clone(), asks })
        })
    }

    /// It has had the server `id` by `by`, if it's `run`'s.
    fn has_had(&mut self, run: RunId, id: &ToolServerId, by: ToolSignInId) {
        if self.run == run {
            self.had.set(id.clone(), by);
        }
    }

    /// Those to ask the run to connect to again, as it says they are. It counts as having had each by
    /// `signed_in` from now on, whatever asking comes to, so it's asked once for each sign-in. One it's connected to is
    /// had by it without being asked, since its calls go with the token there is.
    fn take_due(&mut self, signed_in: &SignedInBy, said: &[ToolServerState]) -> Vec<(ToolServerId, ToolServerState)> {
        let newly: Vec<_> = self.newly(signed_in, said).collect();
        let mut due = Vec::new();
        for Newly { id, by, server, asks } in newly {
            self.had.set(id.clone(), by);
            if asks {
                due.push((id, server));
            }
        }
        due
    }
}

impl LiveState {
    /// `run` has had the server `id` by `by`, as if it had been asked to connect to it again for that.
    pub(super) fn has_had(&mut self, run: RunId, id: &ToolServerId, by: ToolSignInId) {
        if let Some(had) = &mut self.had_sign_ins {
            had.has_had(run, id, by);
        }
    }
}

impl LiveSession {
    /// Has the run asked to connect again to what's due, if anything it has is newly signed in to, after whatever was asked
    /// of the session before.
    pub(crate) fn wake_reconnects(&self) {
        let Ok(hub) = self.hub() else { return };
        let Some(link) = hub.running_worker(&self.workspace_id) else { return };
        if self.of_the_run(&hub, &link, |had, signed_in, said| had.newly(signed_in, said).next().is_some())
            == Some(true)
        {
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

    /// Asks the run to connect again to each server due, on the link whose worker has the token of it. What comes of it
    /// is what the run says of its servers, and what asking comes to is only logged: a run that has gone has none, and
    /// the next is given the server as it's signed in to.
    pub(super) async fn reconnect_tool_servers(&self) {
        let Ok(hub) = self.hub() else { return };
        let Some(link) = hub.running_worker(&self.workspace_id) else { return };
        for (run, id, server) in self.take_due(&hub, &link) {
            let control = Control::Reconnect { server: server.name.clone() };
            if !self.run_claims().is_some_and(|claims| control.allowed(&claims)) {
                continue;
            }
            info!(
                name: "session.tool_server_reconnecting",
                session_id = %self.id,
                server_id = %id,
                "Asking an agent to connect to a tool server that's been signed in to"
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
    }

    /// The servers the run is to be asked to connect to again, and the run, as [`HadSignIns::take_due`] says.
    fn take_due(&self, hub: &Hub, link: &WorkerLink) -> Vec<(RunId, ToolServerId, ToolServerState)> {
        let due = self.of_the_run(hub, link, |had, signed_in, said| {
            let due = had.take_due(signed_in, said);
            due.into_iter().map(|(id, server)| (had.run, id, server)).collect()
        });
        due.unwrap_or_default()
    }

    /// What `read` makes of what the current run has had, what's signed in to on `link`, and what the run says it has, if
    /// it's on that link and has said.
    fn of_the_run<T>(
        &self,
        hub: &Hub,
        link: &WorkerLink,
        read: impl FnOnce(&mut HadSignIns, &SignedInBy, &[ToolServerState]) -> T,
    ) -> Option<T> {
        let signed_in = link.signed_in().borrow().clone();
        let (run, said) = self.said_of_its_servers(hub, link)?;
        let mut said = said?;
        let mut state = self.state.lock();
        state.agent.filter(|agent| agent.run == run)?;
        if let Some(reloading) = &state.reloading {
            reloading.starting(run, &mut said);
        }
        let had = state.had_sign_ins.as_mut().filter(|had| had.run == run)?;
        Some(read(had, &signed_in, &said))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn had(had: SignedInBy) -> HadSignIns {
        HadSignIns::new(RunId::new(1), had)
    }

    fn by(sign_in: &str) -> SignedInBy {
        let mut signed_in = SignedInBy::default();
        signed_in.set("t".into(), sign_in.into());
        signed_in
    }

    fn says(hubs: Option<&str>, state: ServerState) -> Vec<ToolServerState> {
        vec![ToolServerState {
            name: "Linear".into(),
            hubs: hubs.map(Into::into),
            connected_to: None,
            state,
            error: None,
            tools: None,
        }]
    }

    fn says_hubs(state: ServerState) -> Vec<ToolServerState> {
        says(Some("t"), state)
    }

    #[test]
    fn a_server_that_failed_is_due_once_for_each_sign_in_since_the_launch() {
        let mut had = had(by("first"));
        let failed = says_hubs(ServerState::Failed);

        assert_eq!(had.take_due(&by("first"), &failed), [], "as it launched, and as it's renewed");
        assert_eq!(had.take_due(&SignedInBy::default(), &failed), [], "it needs signing in to");
        assert_eq!(had.take_due(&by("second"), &failed), [("t".into(), failed[0].clone())], "as the run has it");
        assert_eq!(had.take_due(&by("second"), &failed), [], "asked once");
        assert_eq!(had.take_due(&by("third"), &says_hubs(ServerState::NeedsSignIn)).len(), 1);
    }

    #[test]
    fn a_server_the_run_is_connected_to_or_turned_off_or_hasnt_settled_isnt_due() {
        let mut waiting = had(SignedInBy::default());
        assert_eq!(waiting.take_due(&by("first"), &[]), []);
        for state in [ServerState::Unknown, ServerState::Starting, ServerState::Disabled] {
            assert_eq!(waiting.take_due(&by("first"), &says_hubs(state)), [], "{state:?}");
        }
        assert_eq!(waiting.take_due(&by("first"), &says_hubs(ServerState::Failed)).len(), 1, "once it says it failed");

        let mut connected = had(SignedInBy::default());
        assert_eq!(connected.take_due(&by("first"), &says_hubs(ServerState::Connected)), []);
        assert_eq!(connected.take_due(&by("first"), &says_hubs(ServerState::Failed)), [], "it had it by that sign-in");
    }

    #[test]
    fn a_server_thats_the_clis_own_isnt_due_whatever_its_called() {
        let mut had = had(SignedInBy::default());

        assert_eq!(had.take_due(&by("first"), &says(None, ServerState::Failed)), []);
    }
}
