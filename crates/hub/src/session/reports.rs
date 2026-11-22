//! What the agent reports that isn't an event, which the session keeps where each report's keeper says.

use std::sync::Arc;

use protocol::capability::Report;
use protocol::capability::account::Limit;
use protocol::capability::output::TodoList;
use protocol::model::{ContextUsage, RunId, SessionId};
use tracing::{debug, error};

use super::LiveSession;
use super::live::LiveState;
use crate::store::{Store, StoreError};

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
                let write =
                    |store: &Store, id: &SessionId, context: &ContextUsage| store.set_session_context(id, context);
                self.keep_on_record(run, context, |state| &mut state.context, write, "how full the agent's context is")
                    .await;
            }
            Report::Todos(todos) => {
                let write = |store: &Store, id: &SessionId, todos: &TodoList| store.set_session_todos(id, todos);
                self.keep_on_record(run, todos, |state| &mut state.todos, write, "the agent's to-do list").await;
            }
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

    /// Keeps `said`, which the agent of `run` reported, on the session's record, where `kept` finds it in the session's
    /// state and `write` records it, and announces it. Only the current run speaks for the session, and only a change is
    /// kept and announced, so the feed isn't told what it knows.
    async fn keep_on_record<T: PartialEq + Clone + Send + Sync + 'static>(
        &self,
        run: RunId,
        said: T,
        kept: impl Fn(&mut LiveState) -> &mut Option<T>,
        write: impl FnOnce(&Store, &SessionId, &T) -> Result<(), StoreError> + Send + 'static,
        what: &str,
    ) {
        let Ok(hub) = self.hub() else { return };
        {
            let mut state = self.state.lock();
            let current = state.agent.map(|agent| agent.run);
            if !says_something_new(current, kept(&mut state).as_ref(), run, &said) {
                return;
            }
        }
        let (id, recorded) = (self.id.clone(), said.clone());
        if let Err(e) = hub.blocking(move |store| write(store, &id, &recorded)).await {
            error!(session = %self.id, "could not record {what}: {e}");
            return;
        }
        *kept(&mut self.state.lock()) = Some(said);
        if let Err(e) = self.announce(&hub).await {
            error!(session = %self.id, "could not announce {what}: {e}");
        }
    }
}

/// Whether what `run` said changes what the session's record says. A run that isn't the one going is one that's been
/// replaced or has exited, and what it says is of a conversation the session has moved on from.
pub(super) fn says_something_new<T: PartialEq>(
    current: Option<RunId>,
    known: Option<&T>,
    run: RunId,
    said: &T,
) -> bool {
    current == Some(run) && known != Some(said)
}
