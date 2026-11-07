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
use protocol::event::ApprovalDecision;
use protocol::model::ApprovalId;
use protocol::worker::AgentSpec;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::process::ChildStdin;
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use crate::{AgentError, AgentOptions, AgentOutput, AgentProcess};

mod launch;
mod output;
mod supervise;
#[cfg(test)]
mod tests;

pub use launch::{SCRUBBED_ENV, build_args};
use launch::{launch, resolve_binary};
use supervise::supervise;

/// A CLI that's still starting runs the user's hooks first, which can take a while.
const CONTROL_REPLY_TIMEOUT: Duration = Duration::from_secs(20);

const STOP_GRACE: Duration = Duration::from_secs(3);
/// How long to wait for buffered output after the process has exited. A tool that outlives the CLI
/// can hold the pipe open forever.
const DRAIN_GRACE: Duration = Duration::from_secs(5);
const STDERR_TAIL_LINES: usize = 20;

struct Shared {
    /// Also held while a prompt is noted and written, and while a relaunch swaps this and replays.
    /// Either a prompt is on the list before the swap or it's written after, never both.
    stdin: tokio::sync::Mutex<Option<ChildStdin>>,
    pid: AtomicU32,
    /// True once the CLI has reported `init`, meaning the conversation opened. Before that, output
    /// is held back and prompts are kept, because the launch may still need retrying.
    confirmed: AtomicBool,
    held_output: Mutex<Vec<AgentOutput>>,
    unconfirmed_prompts: Mutex<Vec<String>>,
    /// Tool inputs waiting on an answer. "Allow" must send the input back to the CLI.
    pending_inputs: Mutex<HashMap<ApprovalId, Value>>,
    /// Control requests of ours waiting on the CLI's answer. Emptied when the process exits, which
    /// tells whoever's waiting.
    pending_controls: Mutex<HashMap<String, oneshot::Sender<Result<(), String>>>>,
    stop_requested: AtomicBool,
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

pub fn spawn(
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
) -> Result<Arc<dyn AgentProcess>, AgentError> {
    let bin = resolve_binary(&options.binary).ok_or(AgentError::NotInstalled)?;
    spawn_at(bin, spec, output, options)
}

/// Runs a specific binary, without looking for one.
pub fn spawn_at(
    bin: PathBuf,
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
) -> Result<Arc<dyn AgentProcess>, AgentError> {
    let launched = launch(&bin, &spec, spec.resume, &options)?;
    let shared = Arc::new(Shared {
        stdin: tokio::sync::Mutex::new(Some(launched.stdin)),
        pid: AtomicU32::new(launched.pid),
        confirmed: AtomicBool::new(false),
        held_output: Mutex::new(Vec::new()),
        unconfirmed_prompts: Mutex::new(Vec::new()),
        pending_inputs: Mutex::new(HashMap::new()),
        pending_controls: Mutex::new(HashMap::new()),
        stop_requested: AtomicBool::new(false),
        gone: AtomicBool::new(false),
        exited: tokio::sync::watch::channel(false).0,
    });
    tokio::spawn(supervise(
        bin,
        spec,
        options,
        launched.child,
        launched.stdout,
        launched.stderr,
        shared.clone(),
        output,
    ));
    Ok(Arc::new(ClaudeProcess { shared }))
}

fn user_message(text: &str) -> Value {
    json!({
        "type": "user",
        "session_id": "",
        "parent_tool_use_id": null,
        "message": {"role": "user", "content": [{"type": "text", "text": text}]},
    })
}

async fn write_to(stdin: &mut Option<ChildStdin>, value: &Value) -> Result<(), AgentError> {
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    let stdin = stdin.as_mut().ok_or(AgentError::Exited)?;
    stdin.write_all(&line).await?;
    stdin.flush().await?;
    Ok(())
}

/// Sends a control request and waits for the CLI's answer.
async fn control(shared: &Shared, request: Value) -> Result<(), AgentError> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let (answered, answer) = oneshot::channel();
    shared.pending_controls.lock().insert(request_id.clone(), answered);
    let line = json!({"type": "control_request", "request_id": request_id, "request": request});
    let result = match write_line(shared, &line).await {
        Ok(()) => tokio::time::timeout(CONTROL_REPLY_TIMEOUT, answer).await,
        Err(e) => {
            shared.pending_controls.lock().remove(&request_id);
            return Err(e);
        }
    };
    match result {
        Ok(Ok(Ok(()))) => Ok(()),
        Ok(Ok(Err(refusal))) => Err(AgentError::Refused(refusal)),
        Ok(Err(_)) => Err(AgentError::Exited),
        Err(_) => {
            shared.pending_controls.lock().remove(&request_id);
            Err(AgentError::NoAnswer)
        }
    }
}

async fn write_line(shared: &Shared, value: &Value) -> Result<(), AgentError> {
    write_to(&mut *shared.stdin.lock().await, value).await
}

#[async_trait]
impl AgentProcess for ClaudeProcess {
    fn pid(&self) -> u32 {
        self.shared.pid.load(Ordering::SeqCst)
    }

    fn is_gone(&self) -> bool {
        self.shared.gone.load(Ordering::SeqCst)
    }

    async fn send_prompt(&self, text: &str) -> Result<(), AgentError> {
        let mut stdin = self.shared.stdin.lock().await;
        let confirmed = self.shared.confirmed.load(Ordering::SeqCst);
        if !confirmed {
            self.shared.unconfirmed_prompts.lock().push(text.to_string());
        }
        match write_to(&mut stdin, &user_message(text)).await {
            // A failed launch closes stdin. The supervisor replays this prompt if it relaunches.
            Err(e) if !confirmed => {
                tracing::debug!("prompt not delivered before the agent opened: {e:#}");
                Ok(())
            }
            Err(e) => {
                // Its stdin is broken, so it can't be told anything more. If it's somehow still
                // running, it mustn't be working alongside the replacement that's coming.
                self.shared.gone.store(true, Ordering::SeqCst);
                signal_group(self.pid(), Signal::Terminate);
                Err(e)
            }
            Ok(()) => Ok(()),
        }
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

    async fn configure(&self, model: Option<&str>, permission_mode: Option<&str>) -> Result<(), AgentError> {
        if let Some(model) = model {
            control(&self.shared, json!({"subtype": "set_model", "model": model})).await?;
        }
        if let Some(mode) = permission_mode {
            control(&self.shared, json!({"subtype": "set_permission_mode", "mode": mode})).await?;
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
