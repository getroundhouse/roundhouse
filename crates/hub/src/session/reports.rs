//! What the agent reports that isn't an event, which the session keeps where each report's keeper says.

use std::sync::Arc;

use protocol::capability::Report;
use protocol::capability::account::Limit;
use protocol::model::{ContextUsage, RunId};
use tracing::{debug, error};

use super::LiveSession;

impl LiveSession {
    /// Keeps what the agent of `run` reported, unless it's `held_to_claims` and the run doesn't claim what it needs.
    /// Only the run going speaks for the session, as each report's own handling checks. One this hub has nowhere to
    /// keep yet is dropped, as one it doesn't know is.
    pub(super) async fn take_report(self: &Arc<Self>, run: RunId, report: Report, held_to_claims: bool) {
        if held_to_claims && !self.state.lock().run_claims().is_some_and(|claims| report.allowed(claims)) {
            debug!(session = %self.id, %run, "dropping a report its run doesn't claim: {report:?}");
            return;
        }
        match report {
            Report::Context { context } => self.note_context(run, context).await,
            // Sent when the hub took it, by the hub's clock, less how long the model took to start answering.
            Report::Cache { request } => self.note_cache(run, request, crate::store::now_ms()).await,
            // Heard when the hub took it, by the hub's clock, as an event the agent said it in is timed.
            Report::Limits { limits, every_models_week } => {
                let limits = limits.iter().map(Limit::usage).collect();
                self.heard_limits(run, limits, every_models_week, crate::store::now_ms());
            }
            Report::Commands { .. }
            | Report::Skills { .. }
            | Report::Personas { .. }
            | Report::Styles { .. }
            | Report::Models { .. }
            | Report::Speed { .. }
            | Report::Aside { .. }
            | Report::ThinkingProgress { .. }
            | Report::Todos(_)
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

    /// The agent said how full its context is. Only the current run speaks for the session, and only a change is kept
    /// and announced, so the feed isn't told what it knows.
    async fn note_context(&self, run: RunId, context: ContextUsage) {
        let Ok(hub) = self.hub() else { return };
        {
            let state = self.state.lock();
            let current = state.agent.map(|agent| agent.run);
            if !says_something_new(current, state.context.as_ref(), run, &context) {
                return;
            }
        }
        let (id, kept) = (self.id.clone(), context.clone());
        if let Err(e) = hub.blocking(move |store| store.set_session_context(&id, &kept)).await {
            error!(session = %self.id, "could not record how full the agent's context is: {e}");
            return;
        }
        self.state.lock().context = Some(context);
        if let Err(e) = self.announce(&hub).await {
            error!(session = %self.id, "could not announce how full the agent's context is: {e}");
        }
    }
}

/// Whether what `run` said of its context changes what the session's record says. A run that isn't the one going is
/// one that's been replaced or has exited, and what it says is of a conversation the session has moved on from.
pub(super) fn says_something_new(
    current: Option<RunId>,
    known: Option<&ContextUsage>,
    run: RunId,
    said: &ContextUsage,
) -> bool {
    current == Some(run) && known != Some(said)
}
