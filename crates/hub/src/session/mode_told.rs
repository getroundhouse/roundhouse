//! Telling the run how the person talks with the agent: as it launches, and as conversation mode is turned on or off,
//! which an adapter that says lines for the agent needs, as [`protocol::capability::output::CoverLine`] says.

use protocol::capability::Control;
use protocol::worker::Delivery;
use tracing::warn;

use super::LiveSession;
use super::messages::Command;

impl LiveSession {
    /// Asks for the run to be told the mode, after whatever was asked of it before, as [`Command::TellTheMode`] says.
    pub(super) fn wake_the_mode(&self) {
        if let Err(e) = self.commands.try_send(Command::TellTheMode) {
            warn!(session = %self.id, "couldn't ask for the agent to be told the mode: {e}");
        }
    }

    /// Tells the run how the person talks with the agent, unless it was told already, or doesn't claim to say lines
    /// for it, or none is running, since a launch says it.
    pub(super) async fn tell_the_mode(&self) {
        let Ok(hub) = self.hub() else { return };
        let (run, mode, told) = {
            let state = self.state.lock();
            (state.agent.map(|agent| agent.run), state.interaction.mode(), state.mode_told)
        };
        let Some(run) = run.filter(|run| told != Some((*run, mode))) else { return };
        let control = Control::Interaction { mode };
        let Some(claims) = self.run_claims().filter(|claims| control.allowed(claims)) else { return };
        let Some(link) = hub.existing_worker(&self.workspace_id) else { return };
        match self.control_claimed(&link, control, &claims).await {
            Ok(Delivery::Delivered | Delivery::AfterTheTurn) => {
                let mut state = self.state.lock();
                if state.agent.is_some_and(|agent| agent.run == run) {
                    state.mode_told = Some((run, mode));
                }
            }
            Ok(Delivery::AgentNotRunning) => {}
            Err(e) => warn!(session = %self.id, %run, "couldn't tell the agent the mode: {e}"),
        }
    }
}
