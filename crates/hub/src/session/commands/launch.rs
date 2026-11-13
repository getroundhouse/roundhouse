//! Starting the session's agent: finding or starting its workspace's worker, and launching the agent there.

use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode};
use protocol::event::{EventBody, ExitReason};
use protocol::worker::{AgentSpec, AgentStarted};
use tracing::warn;

use super::super::{AgentRun, Command, LiveSession, Origin};
use crate::hub::Hub;
use crate::logins::Handed;
use crate::store::Store;
use crate::worker_link::WorkerLink;

/// What an agent is launched with, besides what the session keeps.
struct Launch {
    cwd: String,
    env: Vec<(String, String)>,
    /// The login the hub gives it, as [`Hub::login_for`] chose, or `None` to run it under its machine's own.
    login: Option<Handed>,
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
        let repo = hub.repo_of(&workspace).await?;
        let mut env = crate::hub::workspace_env(&workspace, &repo);
        env.push(("CONDUCTOR_SESSION_ID".to_string(), self.id.to_string()));
        let _nothing_changes_meanwhile = self.configuring.lock().await;
        if self.is_closed() {
            return Err(ApiError::new(ErrorCode::Conflict, "this chat is closed"));
        }
        let login = hub.login_for(&workspace, self.agent).await?;
        self.start_agent(&hub, &link, Launch { cwd, env, login }).await?;
        Ok(link)
    }

    /// Has the agent started, if it's waiting on nothing but a wait, once what's asked of the session before this is
    /// done.
    pub(crate) fn wake_agent(&self) {
        if let Err(e) = self.commands.try_send(Command::WakeAgent) {
            warn!(session = %self.id, "couldn't ask for the agent to be started: {e}");
        }
    }

    /// Starts the agent for a chat that's idle or at work only in the background, with its queue not held. It needs no
    /// prompt: a CLI started again lists its tasks again and asks after them, and starts a turn by itself once one has
    /// ended, as `docs/claude-cli.md` says. One that was cut off, or held for the person, is left for them, and hears
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
                warn!(session = %self.id, "couldn't take a wait that's over off the chat's list: {e}");
            }
            return;
        }
        if let Err(e) = self.ensure_agent().await {
            warn!(session = %self.id, "couldn't start the agent to hear how its wait ended: {e}");
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
        let Launch { cwd, env, login } = launch;
        let (resume, settings) = {
            let state = self.state.lock();
            (state.agent_opened, state.settings.clone())
        };
        let asked = hub.blocking(Store::next_run).await?;
        if let Some(Handed { account, .. }) = &login {
            // Before the launch is sent, so no agent runs under a login the hub gave it without the hub's record of
            // whose it is, since the token doesn't say, and a hub that stops after sending it can't learn it later.
            let (id, account) = (self.id.clone(), account.clone());
            hub.blocking(move |store| store.record_run_login(&id, asked, &account)).await?;
        }
        let tools_token = hub.tools_for(&self.workspace_id, &self.id, link).await;
        // Before the launch, so the new agent's first events aren't recorded ahead of its start.
        let build = link.said_build().map(str::to_string);
        let previous_build = self.previous_build(hub, build.as_deref()).await;
        let spec = AgentSpec {
            session_id: self.id.clone(),
            run: asked,
            agent: self.agent,
            cwd,
            agent_session_id: self.agent_session_id.clone(),
            resume,
            settings,
            env,
            tools_token,
            login: login.map(|handed| handed.login),
        };
        let starting = AgentRun { link: link.id, run: asked };
        self.state.lock().agent = Some(starting);
        let AgentStarted { pid, run, already_running } = link.call(spec).await.inspect_err(|_| {
            let mut state = self.state.lock();
            if state.agent == Some(starting) {
                state.agent = None;
            }
        })?;
        let current = {
            let mut state = self.state.lock();
            // An agent that was already running keeps the run it started with. A new one can exit before
            // this reply arrives, and then its exit has already been recorded, so it isn't made current
            // again.
            let current = already_running || state.agent == Some(starting);
            if current {
                state.agent = Some(AgentRun { link: link.id, run });
            }
            current
        };
        // The worker has said which run the session's agent is, so the logins other runs were given are never asked
        // after. Only on what it says, since a call that failed doesn't say the agent didn't start. One that couldn't be
        // forgotten is only said: its run doesn't start again, so nothing reads it, and the next launch's reply tries
        // again.
        let id = self.id.clone();
        if let Err(e) = hub.blocking(move |store| store.forget_other_run_logins(&id, run)).await {
            warn!(session = %self.id, "couldn't forget the logins earlier launches were given: {}", e.message);
        }
        if !already_running {
            // Whatever is still open was asked by a process that no longer exists.
            self.cancel_approvals(None).await;
            let started = EventBody::AgentStarted { pid, resumed: resume, build, previous_build };
            self.record_or_log(Origin::Run { run, current, numbered: None }, started).await;
        }
        Ok(())
    }

    /// The build the chat's agent last ran on, when it's another than `build`, the one it starts on now. Builds are
    /// compared by their version, as a hub compares a worker's with its own, so every build that isn't stamped is one.
    async fn previous_build(&self, hub: &Hub, build: Option<&str>) -> Option<String> {
        let (build, id) = (build?, self.id.clone());
        match hub.blocking(move |store| store.last_agent_build(&id)).await {
            Ok(last) => last.filter(|last| last != build),
            Err(e) => {
                warn!(session = %self.id, "couldn't read which build the agent last ran on: {}", e.message);
                None
            }
        }
    }
}
