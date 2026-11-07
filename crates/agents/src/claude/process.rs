//! Runs one `claude` process and supervises it across its whole life.
//!
//! The flags, the input format and every behavior relied on here were checked against the real CLI
//! (2.1.319). `docs/claude-cli.md` has the results.

use std::collections::{HashMap, VecDeque};
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::Mutex;
use process::{ProcessRecord, Signal, signal_group};
use protocol::event::{ApprovalDecision, EventBody, NoticeLevel};
use protocol::worker::AgentSpec;
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tracing::warn;

use super::normalize::{Parsed, parse_line};
use crate::{AgentOptions, AgentOutput, AgentProcess};

/// A CLI that's still starting runs the user's hooks first, which can take a while.
const CONTROL_REPLY_TIMEOUT: Duration = Duration::from_secs(20);
/// Overrides how the binary is found. Used by tests, and by anyone pinning a version.
pub(super) const ENV_CLAUDE_BIN: &str = "TRESTLE_CLAUDE_BIN";

const STOP_GRACE: Duration = Duration::from_secs(3);
/// How long to wait for buffered output after the process has exited. A tool that outlives the CLI
/// can hold the pipe open forever.
const DRAIN_GRACE: Duration = Duration::from_secs(5);
const STDERR_TAIL_LINES: usize = 20;

/// Set by a parent Claude session. If the hub was started from inside one, these would make a new
/// agent think it's part of that session. Login and provider settings such as `ANTHROPIC_API_KEY`
/// are the user's own choice, so they pass through.
pub const SCRUBBED_ENV: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_AGENT_SDK_VERSION",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_EFFORT",
    "CLAUDE_PID",
];

pub fn build_args(spec: &AgentSpec, resume: bool) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        "--thinking-display",
        "summarized",
        // Sends approvals to us as control requests. Without it, anything needing approval is denied.
        "--permission-prompt-tool",
        "stdio",
        // Lets full access be chosen later. It doesn't turn it on.
        "--allow-dangerously-skip-permissions",
    ]
    .map(String::from)
    .to_vec();
    args.push(if resume { "--resume" } else { "--session-id" }.to_string());
    args.push(spec.agent_session_id.clone());
    // `default` is what the picker offers for the account's own default, and only the running CLI
    // has a name for it.
    if let Some(model) = spec.model.as_ref().filter(|model| *model != "default") {
        args.extend(["--model".to_string(), model.clone()]);
    }
    if let Some(mode) = &spec.permission_mode {
        args.extend(["--permission-mode".to_string(), mode.clone()]);
    }
    args
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
    unconfirmed_prompts: Mutex<Vec<String>>,
    /// Tool inputs waiting on an answer. "Allow" must send the input back to the CLI.
    pending_inputs: Mutex<HashMap<String, Value>>,
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
) -> anyhow::Result<Arc<dyn AgentProcess>> {
    let bin = resolve_binary().ok_or_else(|| {
        anyhow::anyhow!("could not find the `claude` CLI. Install it, or set {ENV_CLAUDE_BIN} to its path")
    })?;
    spawn_at(bin, spec, output, options)
}

/// Runs a specific binary, without looking for one.
pub fn spawn_at(
    bin: PathBuf,
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
) -> anyhow::Result<Arc<dyn AgentProcess>> {
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

/// `PATH` first, then the places the installers use. A hub started from Finder may have neither
/// `~/.local/bin` nor Homebrew on its `PATH`, and it may not be able to read the login shell's.
fn resolve_binary() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os(ENV_CLAUDE_BIN).filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(path));
    }
    let on_path = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).map(|dir| dir.join("claude")).collect::<Vec<_>>())
        .unwrap_or_default();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let installed = home.iter().flat_map(|home| [home.join(".local/bin/claude"), home.join(".claude/local/claude")]);
    let system = ["/opt/homebrew/bin/claude", "/usr/local/bin/claude"].map(PathBuf::from);
    on_path.into_iter().chain(installed).chain(system).find(|path| is_executable(path))
}

fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

struct Launched {
    child: Child,
    pid: u32,
    stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: ChildStderr,
}

fn launch(bin: &PathBuf, spec: &AgentSpec, resume: bool, options: &AgentOptions) -> anyhow::Result<Launched> {
    let mut command = Command::new(bin);
    command
        .args(build_args(spec, resume))
        .current_dir(&spec.cwd)
        // Reports idle, running and waiting directly. Counting `result` events can't, because a
        // prompt sent mid-turn folds into the running turn and produces no result of its own.
        .env("CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Its own process group, so a signal to the hub's terminal doesn't reach it directly.
        .process_group(0)
        .kill_on_drop(true);
    for name in SCRUBBED_ENV {
        command.env_remove(name);
    }
    command.envs(spec.env.iter().map(|(k, v)| (k, v)));
    let mut child =
        command.spawn().map_err(|e| anyhow::anyhow!("could not start {} in {}: {e}", bin.display(), spec.cwd))?;
    let pid = child.id().unwrap_or_default();
    if let Some(path) = &options.pid_file
        && let Err(e) = ProcessRecord::write(path, pid)
    {
        warn!("could not record agent {pid} in {}: {e}", path.display());
    }
    let (Some(stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        anyhow::bail!("{} started without its pipes", bin.display());
    };
    Ok(Launched { child, pid, stdin, stdout, stderr })
}

#[expect(clippy::too_many_arguments, reason = "its one caller hands over the parts of the process it just started")]
async fn supervise(
    bin: PathBuf,
    spec: AgentSpec,
    options: AgentOptions,
    mut child: Child,
    mut stdout: ChildStdout,
    mut stderr: ChildStderr,
    shared: Arc<Shared>,
    output: mpsc::Sender<AgentOutput>,
) {
    let mut retried = false;
    let (status, stderr_tail) = loop {
        let out_task = tokio::spawn(read_stdout(stdout, shared.clone(), output.clone()));
        let err_task = tokio::spawn(read_stderr(stderr));
        let status = child.wait().await;
        // Before draining, which can take seconds. A launch that hasn't opened may yet be retried,
        // and until that's decided, starting another process would be wrong.
        if shared.confirmed.load(Ordering::SeqCst) {
            shared.gone.store(true, Ordering::SeqCst);
        }
        drain(out_task).await;
        let stderr_tail = drain(err_task).await.unwrap_or_default();

        // The hub decides between --session-id and --resume from its own records, and those can
        // disagree with the CLI's: a launch can crash right after the CLI saved the conversation,
        // or the user can delete it. Either way the CLI exits at once saying so, and the other flag
        // is right.
        let launch_mismatch =
            !shared.confirmed.load(Ordering::SeqCst) && !shared.stop_requested.load(Ordering::SeqCst) && !retried;
        let flip = launch_mismatch.then(|| classify_launch_failure(&stderr_tail)).flatten();
        let Some(failure) = flip else { break (status, stderr_tail) };
        retried = true;
        let resume = matches!(failure, LaunchFailure::AlreadyExists);
        shared.held_output.lock().clear();
        shared.pending_controls.lock().clear();
        if matches!(failure, LaunchFailure::NoConversation) {
            let notice = EventBody::Notice {
                level: NoticeLevel::Warning,
                message: "Claude's saved conversation for this session was not found, so it started a new one. \
                          The history here is intact, but the agent can't see it."
                    .to_string(),
            };
            let _ = output.send(AgentOutput::Event { body: notice, raw: None }).await;
        }
        let launched = match launch(&bin, &spec, resume, &options) {
            Ok(launched) => launched,
            Err(e) => {
                warn!("relaunch failed: {e:#}");
                break (status, vec![format!("{e:#}")]);
            }
        };
        shared.pid.store(launched.pid, Ordering::SeqCst);
        child = launched.child;
        stdout = launched.stdout;
        stderr = launched.stderr;
        let mut stdin = shared.stdin.lock().await;
        *stdin = Some(launched.stdin);
        let prompts = shared.unconfirmed_prompts.lock().clone();
        for prompt in prompts {
            if let Err(e) = write_to(&mut stdin, &user_message(&prompt)).await {
                warn!("could not replay a prompt after relaunching: {e:#}");
            }
        }
    };

    shared.gone.store(true, Ordering::SeqCst);
    *shared.stdin.lock().await = None;
    shared.pending_controls.lock().clear();
    if let Some(path) = &options.pid_file {
        let _ = std::fs::remove_file(path);
    }
    let held: Vec<_> = shared.held_output.lock().drain(..).collect();
    for item in held {
        let _ = output.send(item).await;
    }
    let (code, signal) = status.map_or((None, None), |s| (s.code(), s.signal()));
    let requested = shared.stop_requested.load(Ordering::SeqCst);
    let _ = output.send(AgentOutput::Exited { code, signal, stderr_tail, requested }).await;
    shared.exited.send_replace(true);
}

#[derive(Debug, PartialEq)]
enum LaunchFailure {
    /// `--resume` on a conversation that doesn't exist, so start it with `--session-id`.
    NoConversation,
    /// `--session-id` on a conversation that exists, so continue it with `--resume`.
    AlreadyExists,
}

fn classify_launch_failure(stderr: &[String]) -> Option<LaunchFailure> {
    stderr.iter().find_map(|line| {
        if line.contains("No conversation found") {
            Some(LaunchFailure::NoConversation)
        } else if line.contains("is already in use") {
            Some(LaunchFailure::AlreadyExists)
        } else {
            None
        }
    })
}

async fn drain<T>(mut task: JoinHandle<T>) -> Option<T> {
    if let Ok(result) = tokio::time::timeout(DRAIN_GRACE, &mut task).await {
        result.ok()
    } else {
        task.abort();
        None
    }
}

/// Reads bytes rather than text: one line of invalid UTF-8 mustn't end the whole stream, which would
/// leave the agent blocked and the session stuck.
async fn next_line<R: AsyncBufRead + Unpin>(reader: &mut R, buffer: &mut Vec<u8>) -> Option<String> {
    buffer.clear();
    match reader.read_until(b'\n', buffer).await {
        Ok(0) => None,
        Ok(_) => Some(String::from_utf8_lossy(buffer).trim_end_matches(['\n', '\r']).to_string()),
        Err(e) => {
            warn!("reading agent output failed: {e}");
            None
        }
    }
}

async fn read_stdout(stdout: ChildStdout, shared: Arc<Shared>, output: mpsc::Sender<AgentOutput>) {
    let mut reader = BufReader::new(stdout);
    let mut buffer = Vec::new();
    while let Some(line) = next_line(&mut reader, &mut buffer).await {
        if line.trim().is_empty() {
            continue;
        }
        let item = match parse_line(&line) {
            Parsed::Drop => continue,
            Parsed::ControlReply { request_id, error } => {
                if let Some(waiting) = shared.pending_controls.lock().remove(&request_id) {
                    let _ = waiting.send(error.map_or(Ok(()), Err));
                }
                continue;
            }
            Parsed::Delta(delta) => AgentOutput::Delta(delta),
            Parsed::UnsupportedControl { request_id, subtype } => {
                warn!("answering unsupported control request {subtype} with an error");
                let reply = json!({"type":"control_response","response":{
                    "subtype":"error","request_id":request_id,"error":format!("{subtype} is not supported by this host"),
                }});
                if let Err(e) = write_line(&shared, &reply).await {
                    warn!("could not answer a control request: {e:#}");
                }
                AgentOutput::Event {
                    body: EventBody::AgentOther { kind: format!("control_request/{subtype}") },
                    raw: Some(line),
                }
            }
            Parsed::Event(body) => {
                match &body {
                    EventBody::ApprovalRequested { approval_id, input, .. } => {
                        shared.pending_inputs.lock().insert(approval_id.clone(), input.clone());
                    }
                    EventBody::ApprovalResolved { approval_id, .. } => {
                        shared.pending_inputs.lock().remove(approval_id);
                    }
                    _ => {}
                }
                let opened = matches!(body, EventBody::AgentInit { .. });
                let item = AgentOutput::Event { body, raw: Some(line) };
                if opened && !shared.confirmed.swap(true, Ordering::SeqCst) {
                    shared.unconfirmed_prompts.lock().clear();
                    let held: Vec<_> = shared.held_output.lock().drain(..).collect();
                    for item in held {
                        if output.send(item).await.is_err() {
                            return;
                        }
                    }
                }
                item
            }
        };
        if !shared.confirmed.load(Ordering::SeqCst) {
            shared.held_output.lock().push(item);
        } else if output.send(item).await.is_err() {
            return;
        }
    }
}

async fn read_stderr(stderr: ChildStderr) -> Vec<String> {
    let mut reader = BufReader::new(stderr);
    let mut buffer = Vec::new();
    let mut tail = VecDeque::with_capacity(STDERR_TAIL_LINES);
    while let Some(line) = next_line(&mut reader, &mut buffer).await {
        if tail.len() == STDERR_TAIL_LINES {
            tail.pop_front();
        }
        tail.push_back(line);
    }
    tail.into()
}

fn user_message(text: &str) -> Value {
    json!({
        "type": "user",
        "session_id": "",
        "parent_tool_use_id": null,
        "message": {"role": "user", "content": [{"type": "text", "text": text}]},
    })
}

async fn write_to(stdin: &mut Option<ChildStdin>, value: &Value) -> anyhow::Result<()> {
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    let stdin = stdin.as_mut().ok_or_else(|| anyhow::anyhow!("the agent has exited"))?;
    stdin.write_all(&line).await?;
    stdin.flush().await?;
    Ok(())
}

/// Sends a control request and waits for the CLI's answer.
async fn control(shared: &Shared, request: Value) -> anyhow::Result<()> {
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
        Ok(Ok(Err(refusal))) => Err(anyhow::anyhow!(refusal)),
        Ok(Err(_)) => Err(anyhow::anyhow!("the agent has exited")),
        Err(_) => {
            shared.pending_controls.lock().remove(&request_id);
            Err(anyhow::anyhow!("the agent didn't answer"))
        }
    }
}

async fn write_line(shared: &Shared, value: &Value) -> anyhow::Result<()> {
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

    async fn send_prompt(&self, text: &str) -> anyhow::Result<()> {
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

    async fn resolve_approval(&self, approval_id: &str, decision: &ApprovalDecision) -> anyhow::Result<()> {
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

    async fn configure(&self, model: Option<&str>, permission_mode: Option<&str>) -> anyhow::Result<()> {
        if let Some(model) = model {
            control(&self.shared, json!({"subtype": "set_model", "model": model})).await?;
        }
        if let Some(mode) = permission_mode {
            control(&self.shared, json!({"subtype": "set_permission_mode", "mode": mode})).await?;
        }
        Ok(())
    }

    async fn interrupt(&self) -> anyhow::Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::model::AgentKind;

    fn spec(model: Option<&str>, mode: Option<&str>) -> AgentSpec {
        AgentSpec {
            session_id: "s".into(),
            run: 1,
            agent: AgentKind::Claude,
            cwd: "/tmp".into(),
            agent_session_id: "abc".into(),
            resume: false,
            model: model.map(String::from),
            permission_mode: mode.map(String::from),
            env: vec![],
        }
    }

    fn flag_value(args: &[String], flag: &str) -> Option<String> {
        args.iter().position(|a| a == flag).map(|i| args[i + 1].clone())
    }

    #[test]
    fn first_launch_names_the_conversation_and_later_ones_resume_it() {
        let first = build_args(&spec(None, None), false);
        assert_eq!(flag_value(&first, "--session-id").as_deref(), Some("abc"));
        assert!(!first.contains(&"--resume".to_string()));
        let later = build_args(&spec(None, None), true);
        assert_eq!(flag_value(&later, "--resume").as_deref(), Some("abc"));
        assert!(!later.contains(&"--session-id".to_string()));
    }

    #[test]
    fn with_no_mode_the_users_own_agent_settings_decide() {
        assert!(!build_args(&spec(None, None), false).contains(&"--permission-mode".to_string()));
        let args = build_args(&spec(Some("haiku"), Some("manual")), false);
        assert_eq!(flag_value(&args, "--permission-mode").as_deref(), Some("manual"));
        assert_eq!(flag_value(&args, "--model").as_deref(), Some("haiku"));
    }

    #[test]
    fn full_access_is_available_but_never_on_by_itself() {
        let args = build_args(&spec(None, None), false);
        assert!(args.iter().any(|a| a == "--allow-dangerously-skip-permissions"));
        assert!(!args.iter().any(|a| a == "--dangerously-skip-permissions"));
        assert_eq!(flag_value(&args, "--permission-mode"), None);
    }

    #[test]
    fn the_default_model_has_no_flag() {
        let mut default = spec(None, None);
        default.model = Some("default".to_string());
        assert_eq!(flag_value(&build_args(&default, false), "--model"), None);
        let mut chosen = spec(None, None);
        chosen.model = Some("opus".to_string());
        assert_eq!(flag_value(&build_args(&chosen, false), "--model").as_deref(), Some("opus"));
    }

    #[test]
    fn approvals_always_come_back_to_the_host() {
        let args = build_args(&spec(None, None), false);
        assert_eq!(flag_value(&args, "--permission-prompt-tool").as_deref(), Some("stdio"));
    }

    #[test]
    fn launch_failures_are_told_apart_from_the_clis_messages() {
        let line = |s: &str| vec![s.to_string()];
        assert_eq!(
            classify_launch_failure(&line("No conversation found with session ID: abc")),
            Some(LaunchFailure::NoConversation)
        );
        assert_eq!(
            classify_launch_failure(&line("Error: Session ID abc is already in use.")),
            Some(LaunchFailure::AlreadyExists)
        );
        assert_eq!(classify_launch_failure(&line("Error: something else")), None);
    }
}
