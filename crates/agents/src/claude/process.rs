//! Runs one `claude` process and supervises it across its whole life.
//!
//! The flags, the input format and every behavior relied on here were checked against the real CLI
//! (2.1.319). `docs/claude-cli.md` has the results.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;
use process::{Signal, signal_group};
use protocol::attachment::AttachmentFile;
use protocol::event::ApprovalDecision;
use protocol::model::{ApprovalId, Effort, Seq, SessionSettingsChange};
use protocol::worker::AgentSpec;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::process::ChildStdin;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};

use crate::{AgentError, AgentOptions, AgentOutput, AgentProcess};

mod asking;
mod compact;
mod context;
mod launch;
mod output;
mod plan;
mod prompt;
mod run_files;
mod supervise;
#[cfg(test)]
mod tests;
mod tools;
mod usage;

use compact::{AskedToCompact, ToCompact};
use context::ContextWatch;
use launch::launch;
pub use launch::{SCRUBBED_ENV, build_args};
pub(super) use launch::{command, record, resolve_binary, start};
use prompt::user_message;
pub(super) use prompt::{image_block, user_line};
use run_files::RunFiles;
use supervise::supervise;
use usage::UsageWatch;

/// A CLI that's still starting runs the user's hooks first, which can take a while.
const CONTROL_REPLY_TIMEOUT: Duration = Duration::from_secs(20);

const STOP_GRACE: Duration = Duration::from_secs(3);
/// How long to wait for buffered output after the process has exited. A tool that outlives the CLI
/// can hold the pipe open forever.
const DRAIN_GRACE: Duration = Duration::from_secs(5);
const STDERR_TAIL_LINES: usize = 20;

/// [`DRAIN_GRACE`], unless a test has set `ENV_DRAIN_GRACE_MS`, which only a `testing` build reads.
fn drain_grace() -> Duration {
    #[cfg(feature = "testing")]
    if let Some(ms) = std::env::var(crate::ENV_DRAIN_GRACE_MS).ok().and_then(|ms| ms.parse().ok()) {
        return Duration::from_millis(ms);
    }
    DRAIN_GRACE
}

struct Shared {
    /// Also held while a prompt is noted and written, and while a relaunch swaps this and replays.
    /// Either a prompt is on the list before the swap or it's written after, never both.
    stdin: tokio::sync::Mutex<Option<ChildStdin>>,
    pid: AtomicU32,
    /// True once the CLI has reported `init`, meaning the conversation opened. Before that, output
    /// is held back and prompts are kept, because the launch may still need retrying.
    confirmed: AtomicBool,
    held_output: Mutex<Vec<AgentOutput>>,
    /// Each as the line that was written, ready to write again.
    unconfirmed_prompts: Mutex<Vec<Value>>,
    /// Prompts the CLI hasn't taken yet, by the `uuid` each was sent with, and where each is recorded.
    untaken: Mutex<HashMap<String, Seq>>,
    /// Which of them is a `/compact`, and which one the CLI took, so its compaction can name it.
    compacts: Mutex<AskedToCompact>,
    /// Tool inputs waiting on an answer. "Allow" must send the input back to the CLI.
    pending_inputs: Mutex<HashMap<ApprovalId, Value>>,
    /// Control requests of ours waiting on the CLI's answer. Emptied when the process exits, which
    /// tells whoever's waiting.
    pending_controls: Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>,
    /// Where asking how full the context is has got to.
    context: Mutex<ContextWatch>,
    /// Where asking what's left of the account's limits has got to.
    usage: Mutex<UsageWatch>,
    stop_requested: AtomicBool,
    /// Set from asking a launch into plan mode until the CLI next reports its mode, starts a turn or refuses. `plan` says
    /// what each of those means.
    entering_plan: AtomicBool,
    /// Set as soon as the process is known to be gone, which can be well before `exited`: output is
    /// still being drained and delivered.
    gone: AtomicBool,
    /// `send_replace`, not `send`: a value sent with no receivers is otherwise lost, and a later
    /// `stop` would wait for it forever.
    exited: tokio::sync::watch::Sender<bool>,
}

pub(super) struct ClaudeProcess {
    shared: Arc<Shared>,
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
    let files = RunFiles::of(&options);
    let launched = launch(&bin, &spec, spec.resume, &options)?;
    let shared = Arc::new(Shared {
        stdin: tokio::sync::Mutex::new(Some(launched.stdin)),
        pid: AtomicU32::new(launched.pid),
        confirmed: AtomicBool::new(false),
        held_output: Mutex::new(Vec::new()),
        unconfirmed_prompts: Mutex::new(Vec::new()),
        untaken: Mutex::new(HashMap::new()),
        compacts: Mutex::new(AskedToCompact::default()),
        pending_inputs: Mutex::new(HashMap::new()),
        pending_controls: Mutex::new(HashMap::new()),
        context: Mutex::new(ContextWatch::default()),
        usage: Mutex::new(UsageWatch::default()),
        stop_requested: AtomicBool::new(false),
        entering_plan: AtomicBool::new(false),
        gone: AtomicBool::new(false),
        exited: tokio::sync::watch::channel(false).0,
    });
    plan::start_in(spec.settings.permission_mode, &shared, &mut *shared.stdin.lock().await, &output).await;
    tokio::spawn(supervise(
        bin,
        spec,
        options,
        files,
        launched.child,
        launched.stdout,
        launched.stderr,
        shared.clone(),
        output,
    ));
    Ok(Arc::new(ClaudeProcess { shared }))
}

async fn write_to(stdin: &mut Option<ChildStdin>, value: &Value) -> Result<(), AgentError> {
    let stdin = stdin.as_mut().ok_or(AgentError::Exited)?;
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    stdin.write_all(&line).await?;
    stdin.flush().await?;
    Ok(())
}

/// A control request, registered for its answer before it's written, so an answer can't arrive first and be missed.
struct Asked {
    request_id: String,
    line: Value,
    answer: oneshot::Receiver<Result<Value, String>>,
}

fn ask(shared: &Shared, request: &Value) -> Asked {
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

/// How the CLI refuses a request it doesn't know, as `docs/claude-cli.md` records and a contract test checks.
const UNKNOWN_REQUEST: &str = "Unsupported control request subtype";

/// What the CLI's refusal of a control request was, in its own words: a request it doesn't know, or one it said no
/// to.
fn refused(why: String) -> AgentError {
    if why.contains(UNKNOWN_REQUEST) { AgentError::UnknownRequest(why) } else { AgentError::Refused(why) }
}

/// Sends a control request and waits for the CLI's answer, which is what it sent back, or `null` if nothing.
async fn control(shared: &Shared, request: Value) -> Result<Value, AgentError> {
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
    let asked = super::effort::level(effort).ok_or(AgentError::UnknownEffort)?;
    let settings = json!({"effortLevel": asked});
    control(shared, json!({"subtype": "apply_flag_settings", "settings": settings})).await?;
    let read = control(shared, json!({"subtype": "get_settings"})).await?;
    match super::effort::applied_effort(&read) {
        Some(applied) if applied == asked => Ok(()),
        applied => Err(AgentError::NotApplied { asked, applied: applied.map(String::from) }),
    }
}

async fn write_line(shared: &Shared, value: &Value) -> Result<(), AgentError> {
    write_to(&mut *shared.stdin.lock().await, value).await
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
    match write_to(&mut stdin, &message).await {
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
            process.shared.gone.store(true, Ordering::SeqCst);
            signal_group(process.pid(), Signal::Terminate);
            Err(e)
        }
        Ok(()) => Ok(()),
    }
}

#[async_trait]
impl AgentProcess for ClaudeProcess {
    fn pid(&self) -> u32 {
        self.shared.pid.load(Ordering::SeqCst)
    }

    fn is_gone(&self) -> bool {
        self.shared.gone.load(Ordering::SeqCst)
    }

    async fn send_prompt(&self, prompt: Seq, text: &str, attachments: &[AttachmentFile]) -> Result<(), AgentError> {
        let message = if attachments.is_empty() {
            user_message(text, attachments)
        } else {
            let (text, attachments) = (text.to_string(), attachments.to_vec());
            match tokio::task::spawn_blocking(move || user_message(&text, &attachments)).await {
                Ok(message) => message,
                // As if it had run here.
                Err(e) => std::panic::resume_unwind(e.into_panic()),
            }
        };
        send(self, prompt, message, None).await
    }

    /// Claude is asked by typing `/compact`, with what to keep after it.
    async fn compact(&self, asked: Seq, keeping: Option<&str>) -> Result<(), AgentError> {
        let text = keeping.map_or_else(|| "/compact".to_string(), |keeping| format!("/compact {keeping}"));
        let asked_for = ToCompact { keeping: keeping.map(str::to_string) };
        send(self, asked, user_message(&text, &[]), Some(asked_for)).await
    }

    async fn resolve_approval(&self, approval_id: &ApprovalId, decision: &ApprovalDecision) -> Result<(), AgentError> {
        let original_input = self.shared.pending_inputs.lock().remove(approval_id);
        let response = match decision {
            ApprovalDecision::Allow { updated_input, updated_permissions } => {
                let mut body = json!({
                    "behavior": "allow",
                    "updatedInput": updated_input.clone().or(original_input).unwrap_or_else(|| json!({})),
                });
                if let Some(permissions) = updated_permissions {
                    body["updatedPermissions"] = permissions.clone();
                }
                body
            }
            ApprovalDecision::Deny { message, interrupt } => {
                json!({"behavior": "deny", "message": message, "interrupt": interrupt})
            }
            // The CLI takes these as part of the tool's input. Allowing without them tells the agent
            // that the user didn't answer.
            ApprovalDecision::Answer { answers } => {
                let mut input = original_input.unwrap_or_else(|| json!({}));
                input["answers"] = json!(answers);
                json!({"behavior": "allow", "updatedInput": input})
            }
        };
        write_line(
            &self.shared,
            &json!({"type":"control_response","response":{
                "subtype":"success","request_id":approval_id,"response":response,
            }}),
        )
        .await
    }

    async fn configure(&self, options: &SessionSettingsChange) -> Result<(), AgentError> {
        if let Some(model) = &options.model {
            control(&self.shared, json!({"subtype": "set_model", "model": model.as_str()})).await?;
        }
        if let Some(mode) = options.permission_mode {
            let mode = super::mode::name(mode).ok_or(AgentError::UnknownMode)?;
            control(&self.shared, json!({"subtype": "set_permission_mode", "mode": mode})).await?;
        }
        if let Some(effort) = options.effort {
            set_effort(&self.shared, effort).await?;
        }
        Ok(())
    }

    async fn interrupt(&self) -> Result<(), AgentError> {
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

    async fn stop(&self) {
        self.shared.stop_requested.store(true, Ordering::SeqCst);
        let mut exited = self.shared.exited.subscribe();
        // Already reaped, so its PID may belong to something else by now.
        if *exited.borrow() {
            return;
        }
        // SIGTERM rather than closing stdin, because on EOF the CLI finishes the whole turn first.
        // It stops its tools itself on SIGTERM. They run in their own groups, out of our reach.
        signal_group(self.pid(), Signal::Terminate);
        let wait = exited.wait_for(|done| *done);
        if tokio::time::timeout(STOP_GRACE, wait).await.is_ok() {
            return;
        }
        warn!("agent {} ignored SIGTERM, sending SIGKILL", self.pid());
        signal_group(self.pid(), Signal::Kill);
        let _ = exited.wait_for(|done| *done).await;
    }
}
