use super::super::approvals::Waiting;
use super::launch::resolve_binary;
use super::startup::begin;
use super::{compact, context, prompt, tap, usage};
use crate::run::Run;
use crate::{AgentError, AgentOptions, AgentOutput, AgentProcess};
use async_trait::async_trait;
use compact::{AskedToCompact, ToCompact};
use context::ContextWatch;
use parking_lot::Mutex;
use prompt::user_message;
use protocol::attachment::AttachmentFile;
use protocol::capability::approvals::Decision;
use protocol::capability::settings::{Effort, Settings};
use protocol::model::{ApprovalId, Seq};
use protocol::prompt::Prompt;
use protocol::worker::AgentSpec;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tap::Tap;
use tokio::io::AsyncWriteExt;
use tokio::process::ChildStdin;
use tokio::sync::{mpsc, oneshot};
use tracing::debug;
use usage::UsageWatch;

/// A CLI that's still starting runs the user's hooks first, which can take a while.
const CONTROL_REPLY_TIMEOUT: Duration = Duration::from_secs(20);

/// How long to wait for buffered output after the process has exited. A tool that outlives the CLI
/// can hold the pipe open forever.
const DRAIN_GRACE: Duration = Duration::from_secs(5);

/// [`DRAIN_GRACE`], unless a test has set `ENV_DRAIN_GRACE_MS`, which only a `testing` build reads.
pub(super) fn drain_grace() -> Duration {
    #[cfg(any(test, feature = "testing"))]
    if let Some(ms) = std::env::var(crate::ENV_DRAIN_GRACE_MS).ok().and_then(|ms| ms.parse().ok()) {
        return Duration::from_millis(ms);
    }
    DRAIN_GRACE
}

pub(super) struct Shared {
    /// Also held while a prompt is noted and written, and while a relaunch swaps this and replays.
    /// Either a prompt is on the list before the swap or it's written after, never both.
    pub(super) stdin: tokio::sync::Mutex<Option<ChildStdin>>,
    /// The CLI's group, whether it's gone and whether its stop was asked for, which a relaunch carries on with the new
    /// CLI's pid. Only the run's `Exited` goes out through it. What comes before goes out on the output the readers and
    /// the supervisor hold, once `confirmed` lets it.
    pub(super) run: Run,
    /// True once the CLI has opened the conversation, which its `init` says, and so does its taking or dropping a
    /// prompt, since a launch with the wrong conversation flag exits without reading one. Before that, output is held
    /// back and prompts are kept, because the launch may still need retrying.
    pub(super) confirmed: AtomicBool,
    pub(super) held_output: Mutex<Vec<AgentOutput>>,
    /// Each as the line that was written, ready to write again.
    pub(super) unconfirmed_prompts: Mutex<Vec<Value>>,
    /// Prompts the CLI hasn't taken yet, by the `uuid` each was sent with, and where each is recorded.
    pub(super) untaken: Mutex<HashMap<String, Seq>>,
    /// Which of them is a `/compact`, and which one the CLI took, so its compaction can name it.
    pub(super) compacts: Mutex<AskedToCompact>,
    /// Requests waiting on the person, with the tool's input, which allowing one sends back to the CLI, and what each
    /// asks, which answering one goes by.
    pub(super) waiting: Mutex<HashMap<ApprovalId, Waiting>>,
    /// Control requests of ours waiting on the CLI's answer. Emptied when the process exits, which
    /// tells whoever's waiting.
    pub(super) pending_controls: Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>,
    /// Where asking how full the context is has got to.
    pub(super) context: Mutex<ContextWatch>,
    /// Where asking what's left of the account's limits has got to.
    pub(super) usage: Mutex<UsageWatch>,
    /// Set from asking a launch into plan mode until the CLI next reports its mode, starts a turn or refuses. `plan` says
    /// what each of those means.
    pub(super) entering_plan: AtomicBool,
    /// Told each launch and exit of the CLI, and each line the adapter writes to it and reads from it, in order, for a
    /// conformance trial.
    pub(super) tap: Option<Tap>,
    /// What's said for the agent while it can't answer what was said out loud.
    pub(super) covering: super::covering::Covering,
}

pub(in super::super) struct ClaudeProcess {
    pub(super) shared: Arc<Shared>,
}

pub async fn spawn(
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
) -> Result<Arc<dyn AgentProcess>, AgentError> {
    let bin = resolve_binary(&options.binary).ok_or(AgentError::NotInstalled)?;
    spawn_at(bin, spec, output, options).await
}

/// Runs a specific binary, without looking for one.
pub async fn spawn_at(
    bin: PathBuf,
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
) -> Result<Arc<dyn AgentProcess>, AgentError> {
    begin(bin, spec, output, options, None).await
}

/// Writes `value` as a line to `stdin`, telling `tap` first, since the CLI can answer before the write returns.
pub(super) async fn write_to(
    tap: Option<&Tap>,
    stdin: &mut Option<ChildStdin>,
    value: &Value,
) -> Result<(), AgentError> {
    let stdin = stdin.as_mut().ok_or(AgentError::Exited)?;
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    if let Some(tap) = tap {
        tap.wrote(value);
    }
    stdin.write_all(&line).await?;
    stdin.flush().await?;
    Ok(())
}

/// A control request, registered for its answer before it's written, so an answer can't arrive first and be missed.
pub(super) struct Asked {
    pub(super) request_id: String,
    pub(super) line: Value,
    pub(super) answer: oneshot::Receiver<Result<Value, String>>,
}

pub(super) fn ask(shared: &Shared, request: &Value) -> Asked {
    let request_id = uuid::Uuid::new_v4().to_string();
    let (answered, answer) = oneshot::channel();
    shared.pending_controls.lock().insert(request_id.clone(), answered);
    let line = json!({"type": "control_request", "request_id": request_id, "request": request});
    Asked { request_id, line, answer }
}

/// Waits for the CLI's answer to `asked`, once it has been written: what it sent back, or `null` if nothing.
async fn answered(shared: &Shared, asked: Asked) -> Result<Value, AgentError> {
    match tokio::time::timeout(CONTROL_REPLY_TIMEOUT, asked.answer).await {
        Ok(Ok(Ok(answer))) => Ok(answer),
        Ok(Ok(Err(refusal))) => Err(refused(refusal)),
        Ok(Err(_)) => Err(AgentError::Exited),
        Err(_) => {
            shared.pending_controls.lock().remove(&asked.request_id);
            Err(AgentError::NoAnswer)
        }
    }
}

/// How the CLI refuses a request it doesn't know, as `docs/reference/claude-cli.md` records and a contract test checks.
const UNKNOWN_REQUEST: &str = "Unsupported control request subtype";

/// What the CLI's refusal of a control request was, in its own words: a request it doesn't know, or one it said no
/// to.
pub(super) fn refused(why: String) -> AgentError {
    if why.contains(UNKNOWN_REQUEST) { AgentError::UnknownRequest(why) } else { AgentError::Refused(why) }
}

/// Sends a control request and waits for the CLI's answer, which is what it sent back, or `null` if nothing.
pub(super) async fn control(shared: &Shared, request: Value) -> Result<Value, AgentError> {
    let asked = ask(shared, &request);
    if let Err(e) = write_line(shared, &asked.line).await {
        shared.pending_controls.lock().remove(&asked.request_id);
        return Err(e);
    }
    answered(shared, asked).await
}

/// Changes the effort, and reads it back. The CLI answers success to a level it doesn't know, and changes nothing, so
/// only the level it reports applying says the change took.
async fn set_effort(shared: &Shared, effort: Effort) -> Result<(), AgentError> {
    let asked = super::super::effort::level(&effort).ok_or(AgentError::UnknownEffort)?;
    let settings = json!({"effortLevel": asked});
    control(shared, json!({"subtype": "apply_flag_settings", "settings": settings})).await?;
    let read = control(shared, json!({"subtype": "get_settings"})).await?;
    match super::super::effort::applied_effort(&read) {
        Some(applied) if applied == asked => Ok(()),
        applied => Err(AgentError::NotApplied { asked, applied: applied.map(String::from) }),
    }
}

pub(super) async fn write_line(shared: &Shared, value: &Value) -> Result<(), AgentError> {
    write_to(shared.tap.as_ref(), &mut *shared.stdin.lock().await, value).await
}

/// Writes `message` as what's recorded at `prompt`, so the CLI says when it takes it and when it drops it, and notes
/// it's a compaction someone asked for when it's `asked`.
async fn send(
    process: &ClaudeProcess,
    prompt: Seq,
    mut message: Value,
    asked: Option<ToCompact>,
) -> Result<(), AgentError> {
    // With it, the CLI says when it takes the prompt, and when it drops one it never took.
    let uuid = uuid::Uuid::new_v4().to_string();
    message["uuid"] = json!(uuid);
    let mut stdin = process.shared.stdin.lock().await;
    let confirmed = process.shared.confirmed.load(Ordering::SeqCst);
    if !confirmed {
        process.shared.unconfirmed_prompts.lock().push(message.clone());
    }
    // Before writing, since the CLI can say it took the prompt before the write returns.
    process.shared.untaken.lock().insert(uuid.clone(), prompt);
    if let Some(asked) = asked {
        process.shared.compacts.lock().sent(&uuid, asked);
    }
    match write_to(process.shared.tap.as_ref(), &mut stdin, &message).await {
        // A failed launch closes stdin. The supervisor replays this prompt if it relaunches.
        Err(e) if !confirmed => {
            debug!("prompt not delivered before the agent opened: {e:#}");
            Ok(())
        }
        Err(e) => {
            // Its stdin is broken, so it can't be told anything more. If it's somehow still
            // running, it mustn't be working alongside the replacement that's coming. The prompt
            // goes to that replacement, so this one mustn't say it dropped it.
            process.shared.untaken.lock().remove(&uuid);
            process.shared.compacts.lock().dropped(&uuid);
            process.shared.run.mark_gone();
            process.shared.run.terminate();
            Err(e)
        }
        Ok(()) => Ok(()),
    }
}

impl ClaudeProcess {
    /// Tells the CLI what the person decided of the request `approval_id`, in its own form. One the CLI has no form
    /// for is refused, and the request stays open.
    pub(in super::super) async fn decide(
        &self,
        approval_id: &ApprovalId,
        decision: &Decision,
    ) -> Result<(), AgentError> {
        let response = {
            let mut waiting = self.shared.waiting.lock();
            let response = super::super::approvals::response(decision, waiting.get(approval_id))
                .ok_or_else(|| AgentError::UnknownRequest("Claude's CLI can't take that decision".into()))?;
            waiting.remove(approval_id);
            response
        };
        write_line(
            &self.shared,
            &json!({"type":"control_response","response":{
                "subtype":"success","request_id":approval_id,"response":response,
            }}),
        )
        .await
    }

    /// Changes the model, approval mode or effort of the running CLI to what `settings` name, and resolves once it
    /// has agreed. A setting they leave out is left alone. An error with [`AgentProcess::is_gone`] false is a refusal,
    /// of the first setting refused, and the ones before it may already have been taken.
    /// Sends the prompt recorded at `seq`, as a user message carrying its files. An error means this process didn't take
    /// it.
    pub(in super::super) async fn send_prompt(
        &self,
        seq: Seq,
        prompt: &Prompt<AttachmentFile>,
    ) -> Result<(), AgentError> {
        let message = if prompt.files.is_empty() {
            user_message(prompt)
        } else {
            let prompt = prompt.clone();
            match tokio::task::spawn_blocking(move || user_message(&prompt)).await {
                Ok(message) => message,
                // As if it had run here.
                Err(e) => std::panic::resume_unwind(e.into_panic()),
            }
        };
        send(self, seq, message, None).await?;
        super::covering::Covering::sent(&self.shared, seq, &prompt.text, prompt.spoken);
        Ok(())
    }

    /// The person talks with the agent as `mode` says from now on.
    pub(in super::super) async fn interaction(&self, mode: protocol::model::InteractionMode) {
        super::covering::Covering::mode(&self.shared, mode).await;
    }

    /// Asks it to compact, by typing `/compact`, with what to keep after it. `asked` is where that's recorded, which the
    /// process names when it reports taking or dropping it, and on the compaction.
    pub(in super::super) async fn ask_to_compact(&self, asked: Seq, keeping: Option<&str>) -> Result<(), AgentError> {
        let text = keeping.map_or_else(|| "/compact".to_string(), |keeping| format!("/compact {keeping}"));
        let asked_for = ToCompact { keeping: keeping.map(str::to_string) };
        send(self, asked, user_message(&Prompt::new(text)), Some(asked_for)).await
    }

    /// Stops the turn, and drops the prompts queued for later turns.
    pub(in super::super) async fn interrupt(&self) -> Result<(), AgentError> {
        write_line(
            &self.shared,
            &json!({
                "type": "control_request",
                "request_id": uuid::Uuid::new_v4().to_string(),
                // Stop means stop: messages queued for later turns go too.
                "request": {"subtype": "interrupt", "cancel_queued": true},
            }),
        )
        .await
    }

    pub(crate) async fn change(&self, settings: &Settings) -> Result<(), AgentError> {
        if let Some(model) = &settings.model {
            control(&self.shared, json!({"subtype": "set_model", "model": model.as_str()})).await?;
        }
        if let Some(mode) = settings.mode {
            let mode = super::super::mode::name(mode).ok_or(AgentError::UnknownMode)?;
            control(&self.shared, json!({"subtype": "set_permission_mode", "mode": mode})).await?;
        }
        if let Some(effort) = settings.effort.clone() {
            set_effort(&self.shared, effort).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl AgentProcess for ClaudeProcess {
    fn capabilities(&self) -> protocol::capability::Capabilities {
        super::super::claims::claims()
    }

    async fn control(&self, control: &protocol::capability::Control) -> Result<(), AgentError> {
        super::super::controls::apply(self, control).await
    }

    fn pid(&self) -> u32 {
        self.shared.run.pid()
    }

    fn is_gone(&self) -> bool {
        self.shared.run.is_gone()
    }

    /// Sends SIGTERM rather than closing stdin, because on EOF the CLI finishes the whole turn first. It stops its tools
    /// itself on SIGTERM. They run in their own groups, out of reach of a signal to its own.
    async fn stop(&self) {
        self.shared.run.stop().await;
    }
}

/// As [`spawn_at`], telling `tap` each launch and exit of the CLI, and each line written to it and read from it, in
/// order, for [`crate::conformance`] to capture a trial with.
#[cfg(any(test, feature = "testing"))]
pub async fn spawn_tapped(
    bin: PathBuf,
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
    tap: Tap,
) -> Result<Arc<dyn AgentProcess>, AgentError> {
    begin(bin, spec, output, options, Some(tap)).await
}
