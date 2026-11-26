//! What the agent reports that isn't an event, which the session keeps where each report's keeper says.

use std::sync::Arc;

use protocol::capability::Report;
use protocol::capability::account::Limit;
use protocol::client::ApiError;
use protocol::model::RunId;
use tracing::{debug, error};

use super::LiveSession;
use crate::hub::{Hub, Keeper, Kind, SessionContext, SessionKey, SessionTodos, SessionToolServers};

impl LiveSession {
    /// Keeps what the agent of `run` reported, unless the run doesn't claim what it needs. Only the run going speaks
    /// for the session, as each report's own handling checks. One this hub has nowhere to keep yet is dropped, as one
    /// it doesn't know is.
    pub(super) async fn take_report(self: &Arc<Self>, run: RunId, report: Report) {
        if !self.state.lock().run_claims().is_some_and(|claims| report.allowed(claims)) {
            return self.unclaimed(run, &report);
        }
        match report {
            Report::Context { context } => {
                self.agent_said::<SessionContext>(run, context, "how full the agent's context is").await;
                self.tell_cache_worth_telling().await;
            }
            Report::Todos(todos) => self.agent_said::<SessionTodos>(run, todos, "the agent's to-do list").await,
            Report::ToolServers { mut servers } => {
                self.mark_waiting(run, &mut servers);
                self.run_said_its_own::<SessionToolServers>(run, servers, "the agent's tool servers").await;
                self.said_its_servers(run);
                self.wake_reconnects();
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
            | Report::Hook(_)
            | Report::Credits(_)
            | Report::Outside { .. }
            | Report::Unknown => self.kept_nowhere(&report),
        }
    }

    /// Says in the log that `report`, which `run` doesn't claim, is dropped, as [`Self::kept_nowhere`] does.
    fn unclaimed(&self, run: RunId, report: &Report) {
        let needs = report.needs();
        debug!(
            name: "session.report_unclaimed",
            session_id = %self.id,
            %run,
            ?needs,
            "Dropping a report its run doesn't claim"
        );
    }

    /// Says in the log that `report` is dropped, by what it needs, since the report itself can hold what the agent
    /// said.
    fn kept_nowhere(&self, report: &Report) {
        let needs = report.needs();
        debug!(
            name: "session.report_kept_nowhere",
            session_id = %self.id,
            ?needs,
            "Dropping a report this hub keeps nowhere"
        );
    }

    /// Keeps `said`, which the agent of `run` reported, as the session's observation of kind `K`, as of now, by the
    /// hub's clock. Only the current run speaks for the session, which is asked as it's kept, so what a run said never
    /// lands after the run was replaced. The store tells clients only of a change, so the feed isn't told what it knows.
    async fn agent_said<K: Kind<Of = SessionKey>>(&self, run: RunId, said: K::Value, what: &str) {
        let Ok(hub) = self.hub() else { return };
        let speaks = || self.speaks(run);
        let kept = hub.observation_read_while::<K>(self.observed_as(), said, crate::store::now_ms(), speaks);
        self.kept_or_logged(kept.await, what);
    }

    /// Keeps `said`, as [`Self::agent_said`] does, for what's only true of `run`, as its tool servers are, so `run`
    /// keeps it, and it goes with the run, as [`Self::run_keeps_its_own`] says.
    async fn run_said_its_own<K: Kind<Of = SessionKey>>(&self, run: RunId, said: K::Value, what: &str) {
        let Ok(hub) = self.hub() else { return };
        let (of, keeper, now) = (self.observed_as(), Keeper::Run(run), crate::store::now_ms());
        let kept = hub.observation_said_by_while::<K>(of, keeper, said, now, || self.speaks(run));
        self.kept_or_logged(kept.await, what);
    }

    /// Whether what `run` says speaks for the session now, as [`speaks_for_the_session`] says.
    fn speaks(&self, run: RunId) -> bool {
        speaks_for_the_session(self.state.lock().agent.map(|agent| agent.run), run)
    }

    /// Logs a failure to keep `what` an agent reported.
    fn kept_or_logged(&self, kept: Result<(), ApiError>, what: &str) {
        if let Err(e) = kept {
            error!(
                name: "session.report_unkept",
                session_id = %self.id,
                what,
                error = e.message,
                "Couldn't keep what an agent reported"
            );
        }
    }

    /// Keeps that `run` keeps what's only true of it, its tool servers, from now on, so they go with it, as
    /// `docs/principles/keeping-observations.md` says, as its exit is recorded. Those of the run before go now, as it
    /// starts, rather than once it has, so it's never what it said that goes, and a replaced run's late exit leaves those
    /// of the run after it.
    pub(crate) async fn run_keeps_its_own(&self, hub: &Hub, run: RunId) {
        if let Err(e) = hub.observation_kept_by::<SessionToolServers>(self.observed_as(), Keeper::Run(run)).await {
            error!(
                name: "session.tool_servers_kept",
                session_id = %self.id,
                error = e.message,
                "Couldn't forget the tool servers of the run before"
            );
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
