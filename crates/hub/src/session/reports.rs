//! What the agent reports that isn't an event, which the session keeps where each report's keeper says.

use std::sync::Arc;

use protocol::capability::Report;
use protocol::capability::account::Limit;
use protocol::model::RunId;
use tracing::{debug, error};

use super::LiveSession;
use crate::hub::{Kind, SessionContext, SessionKey, SessionTodos};

impl LiveSession {
    /// Keeps what the agent of `run` reported, unless the run doesn't claim what it needs. Only the run going speaks
    /// for the session, as each report's own handling checks. One this hub has nowhere to keep yet is dropped, as one
    /// it doesn't know is.
    pub(super) async fn take_report(self: &Arc<Self>, run: RunId, report: Report) {
        if !self.state.lock().run_claims().is_some_and(|claims| report.allowed(claims)) {
            debug!(session = %self.id, %run, "dropping a report its run doesn't claim: {report:?}");
            return;
        }
        match report {
            Report::Context { context } => {
                self.agent_said::<SessionContext>(run, context, "how full the agent's context is").await;
                self.tell_cache_worth_telling().await;
            }
            Report::Todos(todos) => self.agent_said::<SessionTodos>(run, todos, "the agent's to-do list").await,
            // Sent when the hub took it, by the hub's clock, less how long the model took to start answering.
            Report::Cache { request } => self.note_cache(run, request, crate::store::now_ms()).await,
            // Heard when the hub took it, by the hub's clock, as an event the agent said it in is timed.
            Report::Limits { limits, every_models_week } => {
                let limits = limits.iter().map(Limit::usage).collect();
                self.heard_limits(run, limits, every_models_week, crate::store::now_ms());
            }
            Report::Covering(line) => self.covering(run, line),
            Report::Covered { wait } => self.covered(run, wait),
            Report::Commands { .. }
            | Report::Skills { .. }
            | Report::Personas { .. }
            | Report::Styles { .. }
            | Report::Models { .. }
            | Report::Speed { .. }
            | Report::Aside { .. }
            | Report::ThinkingProgress { .. }
            | Report::TurnChanges { .. }
            | Report::Rerouted(_)
            | Report::Retrying(_)
            | Report::Retracted(_)
            | Report::Suggestion { .. }
            | Report::CallSummary(_)
            | Report::Notification { .. }
            | Report::CallProgress { .. }
            | Report::Verdict { .. }
            | Report::Denied(_)
            | Report::Rules { .. }
            | Report::Rewound { .. }
            | Report::RewindUndone
            | Report::FilesRestored { .. }
            | Report::WouldRestoreFiles { .. }
            | Report::Goal { .. }
            | Report::Findings { .. }
            | Report::Reply { .. }
            | Report::TaskOutput { .. }
            | Report::Pruned { .. }
            | Report::InstructionFiles { .. }
            | Report::ToolServers { .. }
            | Report::Hook(_)
            | Report::Credits(_)
            | Report::Outside { .. }
            | Report::Unknown => debug!(session = %self.id, "dropping a report this hub keeps nowhere: {report:?}"),
        }
    }

    /// Keeps `said`, which the agent of `run` reported, as the session's observation of kind `K`, as of now, by the
    /// hub's clock. Only the current run speaks for the session, and the store tells clients only of a change, so the
    /// feed isn't told what it knows.
    async fn agent_said<K: Kind<Of = SessionKey>>(&self, run: RunId, said: K::Value, what: &str) {
        let Ok(hub) = self.hub() else { return };
        if !speaks_for_the_session(self.state.lock().agent.map(|agent| agent.run), run) {
            return;
        }
        // Held from the change until the session's revision has moved for its old form, so no copy of the session's
        // record that events announce is read between the two and filled with what the agent said, which would tell a
        // client of version 4 the same twice.
        let _in_order = self.recording.lock().await;
        if let Err(e) = hub.observation_read::<K>(self.observed_as(), said, crate::store::now_ms()).await {
            error!(session = %self.id, "could not keep {what}: {}", e.message);
        }
    }
}

impl LiveSession {
    /// What the hub keeps what it observes of the session by.
    pub(crate) fn observed_as(&self) -> SessionKey {
        SessionKey { workspace_id: self.workspace_id.clone(), session_id: self.id.clone() }
    }
}

/// Whether what `run` says speaks for the session, whose agent's run is `current`. A run that isn't the one going is
/// one that's been replaced or has exited, and what it says is of a conversation the session has moved on from.
pub(super) fn speaks_for_the_session(current: Option<RunId>, run: RunId) -> bool {
    current == Some(run)
}
