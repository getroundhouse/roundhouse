//! Stands in for the `claude` CLI in tests, so they're fast, free and deterministic. It speaks the
//! same stream-json protocol, shaped on output captured from the real CLI, and has the behaviors the
//! worker relies on. `crates/agents/tests/claude/contract.rs` holds the real captures, and
//! `tools/smoke/real-claude.sh` runs the whole stack against the real thing.
//!
//! A prompt is a script: `;`-separated commands, each run in order.
//!   say:TEXT        Stream TEXT in chunks, then send the finished message.
//!   slow:N:TEXT     Same, sending N chunks one per 100 ms, so tests can act mid-stream.
//!   burst:N         Same, sending N numbered chunks as fast as possible.
//!   prose:N:SEED    Say N paragraphs of made-up prose, the same for the same SEED, as fast as possible.
//!   think:N:TEXT    Think, the way `slow` speaks, as the real CLI does with `--thinking-display summarized`.
//!   tool:COMMAND    Ask for approval if the mode requires it, then run COMMAND with `sh -c`.
//!   edit:N          Edit `notes.txt`, changing each of N lines, without asking or touching the disk. The result
//!                   comes back without a patch, so the edit is drawn from what it replaced.
//!   hang:MS         A tool that takes MS milliseconds. Cancelling stops it.
//!   partial:TEXT    Start a message and leave it unfinished, so a crash can cut it off.
//!   late:FILE:TEXT  Leave a process holding stdout open until FILE exists, so an exit takes time to notice,
//!                   then write TEXT as a delta and a `late_marker` line, as an agent that has been replaced
//!                   might. The worker reads for 5 s after an exit.
//!   recall          Say every prompt this conversation has seen, to prove a resume worked.
//!   crash:CODE      Exit immediately with CODE.
//!   env:NAME        Say the value of an environment variable.
//!   effort          Say the effort it's using, as `get_settings` reports it.
//!   keep-effort     From now on, answer a change of effort as if it took, and change nothing.
//!   plan:TEXT       Present a plan for approval, which leaves plan mode if it's approved.
//!   ask:Q?=A/B|*R?=C/D  Ask questions with those options. `*` lets a question take several.
//!   attached        Say what the newest prompt carried besides its script: each image's type and size, each
//!                   file given in the prompt by its name and first line, and each file given by its path by its
//!                   name and size on disk. The script is the prompt's last text block.
//!   background:FILE Start a background task, and go on without it. Once FILE exists it finishes, and the agent
//!                   picks up again by itself to say `finished FILE`, in a turn that starts with `init` alone.
//!                   SIGTERM stops it, and lists none left.
//!   monitor:FILE    The same, started by a `Monitor` call, so it's a monitor.
//!   agent:TEXT      Start a subagent for TEXT in the background, which makes a call and reports a second later.
//!   read:FILE       Read FILE, a PNG, as the real CLI reads an image: as an image block in the result, again in its
//!                   record of the read, and then a line for the model saying how it shrank it.
//!   advise          Ask the advisor, as the real CLI does, and get back a reply encrypted for the model alone.
//!   advise:TEXT     Same, with TEXT as a reply that can be read.
//!   advise-failed:CODE  Same, failing with the error CODE, such as `overloaded`.
//!   advise-unanswered   Same, with no reply at all, as the real CLI's call after one of the agent's own tools gets.
//!
//! A prompt sent mid-turn folds in once the script running finishes, and a prompt with a `uuid` is reported with
//! `command_lifecycle` lines, as the real CLI does. Stopping a turn drops the prompts it hadn't taken, as the worker asks
//! with `cancel_queued`.
//!
//! Conversations persist as files in `$FAKE_CLAUDE_STATE`. `--resume` fails if there isn't one and
//! `--session-id` fails if there is, as with the real CLI.
//!
//! With `--output-format json`, it's the one-off call that titles a prompt instead. See `title.rs`. And as
//! `auth status --json`, it says who's logged in. Its first turn says what's left of the account's limits, as the
//! real CLI's does, and `get_usage` says it whenever it's asked. See `account.rs`.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::needless_pass_by_value,
    reason = "a test program, so it unwraps and takes its values however is shortest"
)]

use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Value, json};

mod account;
mod advisor;
mod background;
mod read;
mod subagent;
mod title;
mod tools;
mod turn;

use turn::run_turn;

type Out = Arc<Mutex<std::io::Stdout>>;

/// Ignores write errors: an orphan keeps working after whoever read its output has gone.
fn emit(out: &Out, value: Value) {
    let mut out = out.lock();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

#[derive(Clone)]
struct Config {
    session_id: String,
    conversation: PathBuf,
    /// Reaches the agent the way stdin does, for what it's told by its own work, such as a task finishing.
    input: mpsc::Sender<Input>,
}

/// What the newest prompt carried besides its script, as `attached` says it.
static LAST_ATTACHED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn attached() -> String {
    let attached = LAST_ATTACHED.lock();
    if attached.is_empty() { "nothing attached".to_string() } else { format!("attached: {}", attached.join(" | ")) }
}

/// A prompt's script, which is its last text block, and what the blocks before it carried.
fn read_prompt(content: &Value) -> String {
    let blocks = content.as_array().cloned().unwrap_or_default();
    let script_at = blocks.iter().rposition(|block| {
        block["type"] == "text" && !block["text"].as_str().unwrap_or_default().starts_with("<attachment ")
    });
    let described = blocks.iter().enumerate().filter(|(i, _)| Some(*i) != script_at).map(|(_, block)| {
        if block["type"] == "image" {
            let data = block["source"]["data"].as_str().unwrap_or_default();
            let bytes = data.len() / 4 * 3 - data.chars().rev().take_while(|c| *c == '=').count();
            format!("{} {bytes} bytes", block["source"]["media_type"].as_str().unwrap_or_default())
        } else {
            describe_tag(block["text"].as_str().unwrap_or_default())
        }
    });
    *LAST_ATTACHED.lock() = described.collect();
    script_at.map(|at| blocks[at]["text"].as_str().unwrap_or_default().to_string()).unwrap_or_default()
}

/// `<attachment name="N" path="P">` with the file inside, or `<attachment name="N" path="P" />` for one to open.
fn describe_tag(text: &str) -> String {
    let mut lines = text.lines();
    let tag = lines.next().unwrap_or_default();
    let attribute = |key: &str| {
        let start = tag.find(&format!("{key}=\"")).map(|at| at + key.len() + 2)?;
        let end = tag[start..].find('"')? + start;
        Some(tag[start..end].replace("&quot;", "\""))
    };
    let name = attribute("name").unwrap_or_default();
    if !tag.ends_with("/>") {
        return format!("{name}: {}", lines.next().unwrap_or_default());
    }
    match attribute("path").map(std::fs::metadata) {
        Some(Ok(on_disk)) => format!("{name}: on disk, {} bytes", on_disk.len()),
        _ => format!("{name}: missing"),
    }
}

/// What a control request changed. Starts as what the flags said.
static LIVE_MODE: Mutex<Option<String>> = Mutex::new(None);
static LIVE_MODEL: Mutex<Option<String>> = Mutex::new(None);

fn current_mode() -> String {
    LIVE_MODE.lock().clone().unwrap_or_else(|| "default".to_string())
}

fn current_model() -> String {
    LIVE_MODEL.lock().clone().unwrap_or_else(|| "fake-model".to_string())
}

/// The real CLI's levels. It ignores any other, with a warning at launch and silently when it's changed.
const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
static LIVE_EFFORT: Mutex<Option<String>> = Mutex::new(None);
/// After `keep-effort`, a change of effort is answered as if it took, and changes nothing.
static KEEPS_EFFORT: Mutex<bool> = Mutex::new(false);

/// With a level it doesn't know, the real CLI takes the user's own settings. This stands in for them.
fn current_effort() -> String {
    LIVE_EFFORT.lock().clone().unwrap_or_else(|| "high".to_string())
}

/// `apply_flag_settings` answers success whatever it's given, as the real CLI does, so only `get_settings` says
/// whether an effort took.
fn apply_flag_settings(settings: &Value) {
    let level = settings["effortLevel"].as_str().filter(|level| EFFORTS.contains(level));
    if let Some(level) = level.filter(|_| !*KEEPS_EFFORT.lock()) {
        *LIVE_EFFORT.lock() = Some(level.to_string());
    }
}

/// Reported the way the real CLI does: in order, and for changes it makes itself.
fn set_mode(out: &Out, mode: &str) {
    *LIVE_MODE.lock() = Some(mode.to_string());
    emit(out, json!({"type":"system","subtype":"status","status":null,"permissionMode":mode}));
}

/// Changes the model, refusing what the real CLI would, and `default`, which it takes as the account's own model, and
/// which the hub never leaves it to choose.
fn set_model(model: &str) -> Result<Value, String> {
    match model {
        "not-a-model" => Err(format!("Model '{model}' not found")),
        "default" => Err("fake claude: asked for `default`".to_string()),
        _ => {
            *LIVE_MODEL.lock() = Some(model.to_string());
            Ok(json!({"model": model}))
        }
    }
}

fn answer_control(out: &Out, request_id: &Value, result: Result<Value, String>) {
    let response = match result {
        Ok(response) => json!({"subtype":"success","request_id":request_id,"response":response}),
        Err(error) => json!({"subtype":"error","request_id":request_id,"error":error}),
    };
    emit(out, json!({"type":"control_response","response":response}));
}

/// A prompt, and the `uuid` it came with, by which it's reported as queued, started, and then completed or cancelled.
struct Prompt {
    text: String,
    uuid: Option<String>,
}

/// Reports where a prompt has got to, as the real CLI does for one sent with a `uuid`.
fn lifecycle(out: &Out, prompt: &Prompt, state: &str) {
    if let Some(uuid) = &prompt.uuid {
        emit(out, json!({"type":"command_lifecycle","command_uuid":uuid,"state":state}));
    }
}

enum Input {
    Prompt(Prompt),
    /// A prompt the agent gives itself, whose turn starts with no state event, as the real CLI's does.
    Resume(Prompt),
    Approval(Value),
    Interrupt,
    Eof,
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).map(|i| args[i + 1].clone())
}

/// Takes the mode, model and effort the flags give, refusing or ignoring what the real CLI would. A launch with no model
/// or effort, which the real CLI would take, is refused, so every test that starts an agent shows the hub always names
/// both.
fn start_as_launched(args: &[String]) {
    for chosen in ["--model", "--effort"] {
        if flag(args, chosen).is_none() {
            eprintln!("fake claude: launched without {chosen}, which leaves it to the CLI");
            std::process::exit(2);
        }
    }
    // The real CLI's flag doesn't list `default`, although that's what it calls the mode once running.
    let launch_mode = flag(args, "--permission-mode");
    if launch_mode.as_deref() == Some("default") {
        eprintln!("error: option '--permission-mode <mode>' argument 'default' is invalid.");
        std::process::exit(1);
    }
    *LIVE_MODE.lock() = launch_mode.map(|mode| if mode == "manual" { "default".to_string() } else { mode });
    *LIVE_MODEL.lock() = flag(args, "--model");
    if let Some(level) = flag(args, "--effort") {
        if EFFORTS.contains(&level.as_str()) {
            *LIVE_EFFORT.lock() = Some(level);
        } else {
            eprintln!("Warning: Unknown --effort value '{level}' — ignoring it and using the default effort.");
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let state = std::env::var("FAKE_CLAUDE_STATE").map(PathBuf::from);
    if args == ["auth", "status", "--json"] {
        println!("{}", account::auth_status(state.as_deref().ok()));
        return;
    }
    let state = state.expect("FAKE_CLAUDE_STATE");
    std::fs::create_dir_all(&state).unwrap();
    if flag(&args, "--output-format").as_deref() == Some("json") {
        title::answer(&state);
        return;
    }
    let out: Out = Arc::new(Mutex::new(std::io::stdout()));

    let session_id = if let Some(id) = flag(&args, "--resume") {
        if !state.join(&id).exists() {
            eprintln!("No conversation found with session ID: {id}");
            emit(
                &out,
                json!({"type":"result","subtype":"error_during_execution","is_error":true,
                "errors":[format!("No conversation found with session ID: {id}")],"session_id":id}),
            );
            std::process::exit(1);
        }
        id
    } else {
        let id = flag(&args, "--session-id").expect("--session-id or --resume");
        if state.join(&id).exists() {
            eprintln!("Error: Session ID {id} is already in use.");
            std::process::exit(1);
        }
        id
    };
    std::fs::write(state.join("last-args"), args.join(" ")).unwrap();
    start_as_launched(&args);
    let (input_tx, input_rx) = mpsc::channel();
    let config = Config { conversation: state.join(&session_id), session_id, input: input_tx.clone() };

    let (control_out, conversation) = (out.clone(), config.conversation.clone());
    std::thread::spawn(move || read_stdin(input_tx, control_out, &conversation));

    // The real CLI finishes its turn on EOF, then exits. Prompts sent mid-turn fold into that turn.
    let mut queued: VecDeque<Prompt> = VecDeque::new();
    let mut eof = false;
    loop {
        let (prompt, by_itself) = match queued.pop_front() {
            Some(prompt) => (prompt, false),
            None if eof => return,
            None => match input_rx.recv() {
                Ok(Input::Prompt(prompt)) => (prompt, false),
                Ok(Input::Resume(prompt)) => (prompt, true),
                Ok(Input::Interrupt | Input::Approval(_)) => continue,
                Ok(Input::Eof) | Err(_) => return,
            },
        };
        run_turn(&config, &out, prompt, by_itself, &input_rx, &mut queued, &mut eof);
    }
}

fn read_stdin(input: mpsc::Sender<Input>, out: Out, conversation: &Path) {
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
        let item = match value["type"].as_str() {
            Some("user") => {
                let prompt = Prompt {
                    text: read_prompt(&value["message"]["content"]),
                    uuid: value["uuid"].as_str().map(str::to_string),
                };
                lifecycle(&out, &prompt, "queued");
                Input::Prompt(prompt)
            }
            Some("control_response") => Input::Approval(value["response"].clone()),
            Some("control_request") => match value["request"]["subtype"].as_str() {
                Some("interrupt") => Input::Interrupt,
                Some("set_permission_mode") => {
                    let mode = value["request"]["mode"].as_str().unwrap_or_default();
                    // The real CLI takes `manual` here too, and calls it `default`.
                    let mode = if mode == "manual" { "default" } else { mode };
                    if ["acceptEdits", "auto", "bypassPermissions", "default", "dontAsk", "plan"].contains(&mode) {
                        answer_control(&out, &value["request_id"], Ok(json!({"mode":mode})));
                        set_mode(&out, mode);
                    } else {
                        answer_control(&out, &value["request_id"], Err("Cannot set permission mode".to_string()));
                    }
                    continue;
                }
                Some("set_model") => {
                    let model = value["request"]["model"].as_str().unwrap_or_default();
                    answer_control(&out, &value["request_id"], set_model(model));
                    continue;
                }
                Some("apply_flag_settings") => {
                    apply_flag_settings(&value["request"]["settings"]);
                    answer_control(&out, &value["request_id"], Ok(Value::Null));
                    continue;
                }
                Some("get_settings") => {
                    let applied = json!({"model": current_model(), "effort": current_effort()});
                    answer_control(&out, &value["request_id"], Ok(json!({"applied": applied})));
                    continue;
                }
                Some("get_context_usage") => {
                    answer_control(&out, &value["request_id"], Ok(account::context_usage(conversation)));
                    continue;
                }
                Some("get_usage") => {
                    answer_control(&out, &value["request_id"], Ok(account::usage(conversation)));
                    continue;
                }
                other => panic!("unexpected control request {other:?}"),
            },
            _ => continue,
        };
        if input.send(item).is_err() {
            return;
        }
    }
    let _ = input.send(Input::Eof);
}

/// Waits without blocking on input, so a turn notices interrupts and later prompts as they arrive.
/// Returns true if the turn should stop.
fn pause(rx: &Receiver<Input>, wait: Duration, queued: &mut VecDeque<Prompt>, eof: &mut bool) -> bool {
    match rx.recv_timeout(wait) {
        Ok(Input::Interrupt) => true,
        Ok(Input::Prompt(prompt) | Input::Resume(prompt)) => {
            queued.push_back(prompt);
            false
        }
        Ok(Input::Eof) => {
            *eof = true;
            false
        }
        Ok(Input::Approval(_)) | Err(RecvTimeoutError::Timeout) => false,
        // recv_timeout returns at once from here on, and the time still has to pass.
        Err(RecvTimeoutError::Disconnected) => {
            *eof = true;
            std::thread::sleep(wait);
            false
        }
    }
}
