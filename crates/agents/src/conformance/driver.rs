//! The driver: what plays a scenario's steps against the adapter, through [`AgentProcess`], as the worker would, and
//! keeps everything the adapter said.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use protocol::event::{ApprovalDecision, EventBody};
use protocol::model::{RunId, Seq};
use protocol::prompt::Prompt;
use tokio::sync::mpsc;

use super::scenario::Failed;
use super::trial::{Seen, Trial};
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
    last_prompt: Seq,
    seen: Vec<Seen>,
    marks: Vec<(&'static str, Instant)>,
}

impl Driver {
    pub fn new(launcher: Box<dyn Launcher>) -> Driver {
        Driver {
            launcher,
            running: None,
            last_run: RunId::new(0),
            last_prompt: Seq::default(),
            seen: Vec::new(),
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
        self.last_run = run;
        self.running = Some(Running { agent, output });
        Ok(())
    }

    fn agent(&self) -> Result<Arc<dyn AgentProcess>, Failed> {
        let running = self.running.as_ref().ok_or_else(|| Failed("a step needs a process, and none runs".into()))?;
        Ok(running.agent.clone())
    }

    pub async fn prompt(&mut self, text: &str) -> Result<(), Failed> {
        self.last_prompt = self.last_prompt.next();
        let seq = self.last_prompt;
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
            if let AgentOutput::Event { body: EventBody::ApprovalRequested { approval_id, .. }, .. } = &output {
                let allow = ApprovalDecision::Allow { updated_input: None, updated_permissions: None };
                running
                    .agent
                    .resolve_approval(approval_id, &allow)
                    .await
                    .map_err(|e| Failed(format!("allowing: {e}")))?;
            }
            let done = matches(&output);
            self.seen.push(Seen { run: self.last_run, at: Instant::now(), output });
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
                Ok(Some(output)) => self.seen.push(Seen { run: self.last_run, at: Instant::now(), output }),
                Ok(None) => return Ok(()),
                Err(_) => return Err(Failed("the agent's output didn't end once it was stopped".into())),
            }
        }
    }

    /// Stops whatever still runs, and hands over what the trial saw, and whether stopping went as it should.
    pub(crate) async fn finish(mut self) -> (Trial, Result<(), Failed>) {
        let stopped = self.stop().await;
        (Trial { seen: self.seen, marks: self.marks }, stopped)
    }
}
