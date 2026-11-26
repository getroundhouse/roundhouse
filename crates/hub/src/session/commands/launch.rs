//! Starting the session's agent: finding or starting its workspace's worker, and launching the agent there.

use std::sync::Arc;

use protocol::capability::Capabilities;
use protocol::capability::conversation::Start;
use protocol::capability::settings::Settings;
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::event::{EventBody, ExitReason};
use protocol::model::{GithubRepositoryName, InteractionMode, RunCapabilities, RunId};
use protocol::worker::{AgentSpec, AgentStarted, AgentStop};
use tracing::{error, warn};

use super::super::chat_settings::ChatSettings;
use super::super::had_servers::HadServers;
use super::super::{AgentRun, Command, LiveSession, Origin};
use crate::hub::Hub;
use crate::logins::{Handed, Launching, NO_ACCOUNT_TO_SPARE};
use crate::store::Store;
use crate::tool_servers::ServersAsSent;
use crate::worker_link::WorkerLink;

/// What an agent is launched with, besides what the session keeps.
struct Launch {
    cwd: String,
    env: Vec<(String, String)>,
    /// The login the hub gives it, as [`Hub::launching`] chose, or `None` to run it under its machine's own.
    login: Option<Handed>,
    /// The workspace's repository on GitHub, which the agent is told, if it's on github.com.
    repository: Option<GithubRepositoryName>,
}

impl LiveSession {
    /// Finds or starts the worker and the agent, and returns the connection to use.
    pub(super) async fn ensure_agent(&self) -> Result<Arc<WorkerLink>, ApiError> {
        let hub = self.hub()?;
        let link = hub.ensure_worker(&self.workspace_id).await?;
        let known = self.state.lock().agent;
        if known.is_some_and(|agent| agent.link == link.id) {
            return Ok(link);
        }
        if let Some(previous) = known {
            // The worker was replaced before its loss reached this session.
            self.agent_lost(previous.link, ExitReason::WorkerLost).await;
        }
        let (workspace, cwd) = hub.ready_workspace(&self.workspace_id).await?.into_parts();
        let mut env = hub.workspace_env(&workspace).await?;
        let repository = hub.github_repository_of(&workspace).await?;
        env.push(("CONDUCTOR_SESSION_ID".to_string(), self.id.to_string()));
        let _nothing_changes_meanwhile = self.configuring.lock().await;
        if self.is_closed() {
            return Err(ApiError::new(ErrorCode::Conflict, "this chat is closed"));
        }
        let on = self.state.lock().login.clone();
        let login = match hub.launching(self.agent, on).await? {
            Launching::Handed(handed) => Some(handed),
            Launching::Ambient => None,
            Launching::NoAccountToSpare => {
                self.needs_an_account(&hub).await?;
                return Err(ApiError::new(ErrorCode::Conflict, NO_ACCOUNT_TO_SPARE));
            }
        };
        self.start_agent(&hub, &link, Launch { cwd, env, login, repository }).await?;
        Ok(link)
    }

    /// Records that the chat couldn't start, since Automatic found no account with usage to spare, and tells every
    /// client, so it says so until the chat is on an account.
    async fn needs_an_account(&self, hub: &Hub) -> Result<(), ApiError> {
        let _in_order = self.recording.lock().await;
        let id = self.id.clone();
        if hub.blocking(move |store| store.needs_an_account(&id)).await? {
            self.announce(hub).await?;
        }
        Ok(())
    }

    /// Has the agent started, if it's waiting on nothing but a wait, once what's asked of the session before this is
    /// done.
    pub(crate) fn wake_agent(&self) {
        if let Err(e) = self.commands.try_send(Command::WakeAgent) {
            warn!(
                name: "session.start_unasked",
                session_id = %self.id,
                error = %e,
                "Couldn't ask for the agent to be started"
            );
        }
    }

    /// Starts the agent for a chat that's idle or at work only in the background, with its queue not held. It needs no
    /// prompt: a CLI started again lists its tasks again and asks after them, and starts a turn by itself once one has
    /// ended, as `docs/reference/claude-cli.md` says. One that was cut off, or held for the person, is left for them, and hears
    /// how its wait ended with their next prompt, since its CLI lists its tasks again then too, so the wait comes off
    /// its list meanwhile, rather than leave it at work on a wait that's over.
    pub(super) async fn handle_wake_agent(&self) {
        let Ok(hub) = self.hub() else { return };
        let waiting_on_nothing = {
            let progress = &self.state.lock().progress;
            progress.status.between_turns() && !progress.queue.is_held()
        };
        if self.agent_running() {
            return;
        }
        if !waiting_on_nothing || self.can_be_asked(&hub).await.is_err() {
            if let Err(e) = self.unlist_waits().await {
                warn!(
                    name: "session.wait_not_cleared",
                    session_id = %self.id,
                    error = %e,
                    "Couldn't take a wait that's over off the chat's list"
                );
            }
            return;
        }
        if let Err(e) = self.ensure_agent().await {
            warn!(
                name: "session.start_failed",
                session_id = %self.id,
                error = %e,
                "Couldn't start the agent to hear how its wait ended"
            );
            self.record_start_failed(&e).await;
        }
    }

    /// Tells every client the agent couldn't be started, or reached, for what was asked of it, and ends what it had
    /// asked approval for, since it won't hear the answer.
    pub(super) async fn record_start_failed(&self, e: &ApiError) {
        self.cancel_approvals(None).await;
        let failed = EventBody::AgentExited {
            reason: ExitReason::StartFailed,
            code: None,
            signal: None,
            stderr_tail: vec![e.message.clone()],
        };
        self.record_or_log(Origin::Hub, failed).await;
    }

    /// Picks the run before the launch and marks it current, so the new agent's first deltas aren't
    /// taken for a replaced agent's and dropped.
    async fn start_agent(&self, hub: &Hub, link: &WorkerLink, launch: Launch) -> Result<(), ApiError> {
        let Launch { cwd, env, login, repository } = launch;
        let (resume, start, kept, interaction) = {
            let state = self.state.lock();
            let start = match (&state.agent_session_id, state.agent_opened) {
                (Some(id), true) => Start::Resume { conversation: id.clone() },
                (id, _) => Start::New { conversation: id.clone() },
            };
            (state.agent_opened, start, state.settings.clone(), state.interaction.mode())
        };
        let settings = self.launched_with(hub, kept)?;
        let asked = hub.blocking(Store::next_run).await?;
        self.record_login(hub, asked, login.as_ref()).await?;
        let tools_token = Some(hub.tools_for(&self.workspace_id, &self.id, link).await?);
        // Before the launch, so the new agent's first events aren't recorded ahead of its start.
        let build = link.said_build().map(str::to_string);
        let previous_build = self.previous_build(hub, build.as_deref()).await;
        let (session_id, agent, login) = (self.id.clone(), self.agent, login.map(|handed| handed.login));
        let spec = AgentSpec {
            session_id,
            run: asked,
            agent,
            cwd,
            start,
            settings,
            env,
            tools_token,
            login,
            repository,
            interaction,
        };
        let starting = AgentRun { link: link.id, run: asked };
        // As of now, so one signed in to or edited while the agent starts is looked at once it has.
        let has = link.tool_servers().borrow().clone();
        self.state.lock().agent = Some(starting);
        self.run_keeps_its_own(hub, asked).await;
        let launched = self.launch(link, spec).await;
        let AgentStarted { pid, run, already_running, capabilities } = launched.inspect_err(|_| {
            let mut state = self.state.lock();
            if state.agent == Some(starting) {
                state.agent = None;
            }
        })?;
        let claims = RunCapabilities { run, claims: capabilities };
        let current = self.take_started(starting, already_running, &claims, has, interaction);
        self.forget_other_run_logins(hub, run).await;
        if already_running {
            // Its start, which recorded them, may be from before the record let go of them, as when its worker's
            // connection dropped.
            self.keep_claims(hub, claims).await;
        } else {
            // Whatever is still open was asked by a process that no longer exists.
            self.cancel_approvals(None).await;
            let capabilities = Some(claims.claims);
            let started = EventBody::AgentStarted { pid, resumed: resume, build, previous_build, capabilities };
            self.record_or_log(Origin::Run { run, current, numbered: None }, started).await;
        }
        // The mode may have changed while it launched, and a tool server been signed in to or edited.
        if current {
            self.wake_the_mode();
            self.wake_reconnects();
        }
        Ok(())
    }

    /// What the agent launches with: the chat's settings, `kept`, but in the mode a chat starts in if its kind doesn't
    /// offer the chat's, which the agent went into by itself, since that can't be asked for, rather than not at all.
    fn launched_with(&self, hub: &Hub, mut kept: ChatSettings) -> Result<Settings, ApiError> {
        let kind = hub.agent(self.agent)?;
        if !kind.modes().contains(&kept.mode)
            && let Some(starts_in) = kind.default_mode()
        {
            kept.mode = starts_in;
        }
        Ok(kept.settings())
    }

    /// Makes the run its worker started current, with its claims, its tool servers as its worker `has` them and the mode it
    /// was told as it started, and says whether it is. An agent that was already running keeps the run it started with,
    /// was told the mode as it was then, and has had its servers as the session has it, if it does. A new one can exit
    /// before its worker's reply arrives, and then its exit has already been recorded, so it isn't made current again.
    fn take_started(
        &self,
        starting: AgentRun,
        already_running: bool,
        claims: &RunCapabilities,
        has: ServersAsSent,
        interaction: InteractionMode,
    ) -> bool {
        let run = claims.run;
        let mut state = self.state.lock();
        let current = already_running || state.agent == Some(starting);
        if current {
            state.agent = Some(AgentRun { link: starting.link, run });
            // With it, so nothing is asked of it that its claims don't allow.
            state.capabilities = Some(claims.clone());
            if !already_running {
                state.had_servers = Some(HadServers::new(run, has));
            } else if !state.had_servers.as_ref().is_some_and(|had| had.is_of(run)) {
                state.had_servers = Some(HadServers::new(run, ServersAsSent::default()));
            }
            state.mode_told = (!already_running).then_some((run, interaction));
        }
        current
    }

    /// Records whose login the run `asked` is given, if the hub gives it one. Before the launch is sent, so no agent
    /// runs under a login the hub gave it without the hub's record of whose it is, since the token doesn't say, and a
    /// hub that stops after sending it can't learn it later.
    ///
    /// A session on no login yet given one is put on the hub's for its kind's default, without another account's cache,
    /// as a move is. The session's copy follows the row at once, before the launch is sent, so every later launch is
    /// under the same login, even once this one has failed. Announced if that changed its record, under the lock events
    /// are recorded under, so the feed hears each copy of the record in the order they were made. One given none is put
    /// on the ambient login only once its worker says whose that is, as filing the account it runs under does.
    async fn record_login(&self, hub: &Hub, asked: RunId, login: Option<&Handed>) -> Result<(), ApiError> {
        let Some(Handed { account, placed, .. }) = login else { return Ok(()) };
        let _in_order = self.recording.lock().await;
        let (id, filed) = (self.id.clone(), account.clone());
        let placed = *placed;
        let recorded = hub.blocking(move |store| store.record_run_login(&id, asked, &filed, placed)).await?;
        self.state.lock().login = recorded.on;
        if recorded.forgot_cache {
            self.cache_forgotten().await;
        }
        if let Some(record) = recorded.account_changed {
            hub.notify(HubNotice::Account(record));
        }
        if recorded.session_changed
            && let Err(e) = self.announce(hub).await
        {
            error!(
                name: "session.login_untold",
                session_id = %self.id,
                error = %e,
                "Couldn't announce which login the chat is on"
            );
        }
        Ok(())
    }

    /// Sends the launch, and returns the worker's reply, unless its run claims what it can't be held to.
    async fn launch(&self, link: &WorkerLink, spec: AgentSpec) -> Result<AgentStarted, ApiError> {
        let started: AgentStarted = link.call(spec).await?;
        self.held_to(link, &started.capabilities).await?;
        Ok(started)
    }

    /// The worker has said which run the session's agent is, so the logins other runs were given are never asked
    /// after. Only on what it says, since a call that failed doesn't say the agent didn't start. One that couldn't be
    /// forgotten is only said: its run doesn't start again, so nothing reads it, and the next launch's reply tries
    /// again.
    async fn forget_other_run_logins(&self, hub: &Hub, run: RunId) {
        let id = self.id.clone();
        if let Err(e) = hub.blocking(move |store| store.forget_other_run_logins(&id, run)).await {
            warn!(
                name: "session.logins_unforgotten",
                session_id = %self.id,
                error = e.message,
                "Couldn't forget the logins earlier launches were given"
            );
        }
    }

    /// Refuses a run whose claims it can't be held to, since a claim that needs another without it means nothing, as
    /// [`Capabilities::unmet_needs`] says, and stops it, so nothing is asked of it or recorded as its. *Unconfirmed.*
    async fn held_to(&self, link: &WorkerLink, claims: &Capabilities) -> Result<(), ApiError> {
        let unmet = claims.unmet_needs();
        if unmet.is_empty() {
            return Ok(());
        }
        warn!(
            name: "session.run_refused",
            session_id = %self.id,
            ?unmet,
            "Refusing a run that claims what needs claims it doesn't make"
        );
        // Only said, since the launch fails either way, and the run isn't made current, so nothing it says is taken as
        // the session's.
        if let Err(e) = link.call(AgentStop { session_id: self.id.clone() }).await {
            warn!(
                name: "session.run_not_stopped",
                session_id = %self.id,
                error = e.message,
                "Couldn't stop a run whose claims it can't be held to"
            );
        }
        let message = "the agent said it can do things that need others it didn't say it can do, so it was stopped";
        Err(ApiError::new(ErrorCode::Worker, message))
    }

    /// Puts `claims` on the session's record, and tells everyone if it didn't hold them already, for a run found
    /// running, unless it has exited since, in order with what's recorded, so its exit finds them to take off. It's a
    /// write outside any event, which repairs a record that let go of what the run's start set, so the claims stay a
    /// record, as `docs/principles/observations.md` says. One that can't be recorded is only said: the run is held to
    /// them all the same, and a client offers what its kind claims until the next launch says them.
    pub(super) async fn keep_claims(&self, hub: &Hub, claims: RunCapabilities) {
        let _in_order = self.recording.lock().await;
        if self.state.lock().agent.is_none_or(|agent| agent.run != claims.run) {
            return;
        }
        let id = self.id.clone();
        match hub.blocking(move |store| store.set_claims(&id, &claims)).await {
            Ok(true) => {}
            // The record held them already, as when a hub carried on with the run.
            Ok(false) => return,
            Err(e) => {
                warn!(
                    name: "session.claims_unrecorded",
                    session_id = %self.id,
                    error = e.message,
                    "Couldn't record what a run found running claims"
                );
                return;
            }
        }
        if let Err(e) = self.announce(hub).await {
            warn!(
                name: "session.claims_unsaid",
                session_id = %self.id,
                error = e.message,
                "Couldn't say what a run found running claims"
            );
        }
    }

    /// The build the chat's agent last ran on, when it's another than `build`, the one it starts on now. Builds are
    /// compared by their version, as a hub compares a worker's with its own, so every build that isn't stamped is one.
    async fn previous_build(&self, hub: &Hub, build: Option<&str>) -> Option<String> {
        let (build, id) = (build?, self.id.clone());
        match hub.blocking(move |store| store.last_agent_build(&id)).await {
            Ok(last) => last.filter(|last| last != build),
            Err(e) => {
                warn!(
                    name: "session.build_unread",
                    session_id = %self.id,
                    error = e.message,
                    "Couldn't read which build the agent last ran on"
                );
                None
            }
        }
    }
}
