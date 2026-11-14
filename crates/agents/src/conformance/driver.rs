//! The driver: what plays a scenario's steps against the adapter, through [`AgentProcess`], as the worker would, and
//! keeps everything the adapter said.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use protocol::capability::Capabilities;
use protocol::event::{ApprovalDecision, ApprovalOutcome, EventBody, SessionEvent};
use protocol::model::{RunId, Seq};
use protocol::prompt::Prompt;
use tokio::sync::mpsc;

use super::scenario::Failed;
use super::trial::Seen;
use crate::{AgentOutput, AgentProcess};

/// What [`Driver::interrupt`] marks, for a check to time from.
pub const INTERRUPTED: &str = "interrupted";

/// How the driver starts each of a scenario's processes, on the real CLI or on a stand-in that replays a capture,
/// and what it can see that the adapter doesn't say.
#[async_trait]
pub trait Launcher: Send {
    /// Starts the process for `run`, a new conversation or resuming the last, which says what it does to `output`.
    async fn launch(
        &mut self,
        run: RunId,
        resume: bool,
        output: mpsc::Sender<AgentOutput>,
    ) -> Result<Arc<dyn AgentProcess>, Failed>;

    /// Waits until something the agent ran has made `name` in its worktree, or until the stand-in replaying a capture
    /// has reached where that was seen.
    async fn made(&mut self, name: &str) -> Result<(), Failed>;

    /// How long a step waits for what the adapter says before it fails.
    fn patience(&self) -> std::time::Duration;
}

struct Running {
    agent: Arc<dyn AgentProcess>,
    output: mpsc::Receiver<AgentOutput>,
}

pub struct Driver {
    launcher: Box<dyn Launcher>,
    running: Option<Running>,
    last_run: RunId,
    seen: Vec<Seen>,
    /// What a hub would have recorded, in its order and numbered as it numbers it: what the driver did as a hub does,
    /// and what the agent said.
    log: Vec<SessionEvent>,
    /// What each run claims.
    claims: Vec<(RunId, Capabilities)>,
    marks: Vec<(&'static str, Instant)>,
}

impl Driver {
    pub fn new(launcher: Box<dyn Launcher>) -> Driver {
        Driver {
            launcher,
            running: None,
            last_run: RunId::new(0),
            seen: Vec::new(),
            log: Vec::new(),
            claims: Vec::new(),
            marks: Vec::new(),
        }
    }

    /// Starts a process with a new conversation.
    pub async fn start(&mut self) -> Result<(), Failed> {
        self.launch(false).await
    }

    /// Starts a process that resumes the conversation the last one had.
    pub async fn resume(&mut self) -> Result<(), Failed> {
        self.launch(true).await
    }

    async fn launch(&mut self, resume: bool) -> Result<(), Failed> {
        if self.running.is_some() {
            return Err(Failed("a step started a process while another ran".into()));
        }
        let (sender, output) = mpsc::channel(1024);
        let run = RunId::new(self.last_run.get() + 1);
        let agent = self.launcher.launch(run, resume, sender).await?;
        let claims = agent.capabilities();
        self.claims.push((run, claims.clone()));
        // As the worker reports a launch, with what the run claims, which the hub records.
        let capabilities = Some(Box::new(claims));
        let started = EventBody::AgentStarted {
            pid: agent.pid(),
            resumed: resume,
            build: None,
            previous_build: None,
            capabilities,
        };
        self.record(Some(run), started);
        self.last_run = run;
        self.running = Some(Running { agent, output });
        Ok(())
    }

    fn agent(&self) -> Result<Arc<dyn AgentProcess>, Failed> {
        let running = self.running.as_ref().ok_or_else(|| Failed("a step needs a process, and none runs".into()))?;
        Ok(running.agent.clone())
    }

    /// Records it as a hub does, and returns where.
    fn record(&mut self, run: Option<RunId>, body: EventBody) -> Seq {
        super::trial::record(&mut self.log, run, body)
    }

    /// Records the prompt, as a hub does before it sends one, and sends it with where it's recorded.
    pub async fn prompt(&mut self, text: &str) -> Result<(), Failed> {
        let seq = self.record(None, EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new(text) });
        self.agent()?.send_prompt(seq, &Prompt::new(text)).await.map_err(|e| Failed(format!("prompting: {e}")))
    }

    /// Interrupts the turn, marking when as [`INTERRUPTED`].
    pub async fn interrupt(&mut self) -> Result<(), Failed> {
        self.mark(INTERRUPTED);
        self.agent()?.interrupt().await.map_err(|e| Failed(format!("interrupting: {e}")))
    }

    /// Notes when something happened, for a check to compare, such as how soon a turn ended after an interrupt.
    pub fn mark(&mut self, name: &'static str) {
        self.marks.push((name, Instant::now()));
    }

    /// Waits until something the agent ran has made `name` in its worktree, such as a command that has started, since
    /// the adapter says nothing as a tool starts.
    pub async fn made(&mut self, name: &str) -> Result<(), Failed> {
        self.launcher.made(name).await
    }

    /// Waits until the adapter says something `matches`, allowing whatever the agent asks meanwhile.
    pub async fn wait(&mut self, what: &str, matches: impl Fn(&AgentOutput) -> bool) -> Result<(), Failed> {
        let deadline = tokio::time::Instant::now() + self.launcher.patience();
        loop {
            let running =
                self.running.as_mut().ok_or_else(|| Failed(format!("waiting for {what} with nothing running")))?;
            let output = match tokio::time::timeout_at(deadline, running.output.recv()).await {
                Ok(Some(output)) => output,
                Ok(None) => return Err(Failed(format!("the agent exited before {what}"))),
                Err(_) => return Err(Failed(format!("timed out waiting for {what}"))),
            };
            let done = matches(&output);
            let asked =
                if let AgentOutput::Event { body: EventBody::ApprovalRequested { approval_id, .. }, .. } = &output {
                    Some(approval_id.clone())
                } else {
                    None
                };
            self.keep(output);
            if let Some(approval_id) = asked {
                // Recorded before it's sent, as the hub records a decision.
                let allowed = EventBody::ApprovalResolved {
                    approval_id: approval_id.clone(),
                    outcome: ApprovalOutcome::Allowed,
                    answers_by_text: None,
                    answers_by_position: None,
                    by_setting: false,
                };
                self.record(None, allowed);
                let allow = ApprovalDecision::Allow { updated_input: None, updated_permissions: None };
                self.agent()?
                    .resolve_approval(&approval_id, &allow)
                    .await
                    .map_err(|e| Failed(format!("allowing: {e}")))?;
            }
            if done {
                return Ok(());
            }
        }
    }

    /// Waits for the turn to end.
    pub async fn turn_ended(&mut self) -> Result<(), Failed> {
        self.wait("the turn to end", |output| {
            matches!(output, AgentOutput::Event { body: EventBody::TurnEnded { .. }, .. })
        })
        .await
    }

    /// Stops the process, as the worker does, and keeps all it said until it exited.
    pub async fn stop(&mut self) -> Result<(), Failed> {
        let Some(mut running) = self.running.take() else { return Ok(()) };
        running.agent.stop().await;
        let deadline = tokio::time::Instant::now() + self.launcher.patience();
        loop {
            match tokio::time::timeout_at(deadline, running.output.recv()).await {
                Ok(Some(output)) => self.keep(output),
                Ok(None) => return Ok(()),
                Err(_) => return Err(Failed("the agent's output didn't end once it was stopped".into())),
            }
        }
    }

    /// Keeps what the adapter said, and logs it as a hub records it: each event under its run, and the process's exit
    /// as the hub's record of it, with its run.
    fn keep(&mut self, output: AgentOutput) {
        let run = self.last_run;
        if let Some(body) = output.recorded() {
            self.record(Some(run), body);
        }
        self.seen.push(Seen { run, at: Instant::now(), output });
    }

    /// Stops whatever still runs, and hands over what the trial saw, and whether stopping went as it should.
    #[cfg(unix)]
    pub(crate) async fn finish(mut self) -> (super::trial::Trial, Result<(), Failed>) {
        let stopped = self.stop().await;
        (super::trial::Trial { seen: self.seen, log: self.log, claims: self.claims, marks: self.marks }, stopped)
    }
}
