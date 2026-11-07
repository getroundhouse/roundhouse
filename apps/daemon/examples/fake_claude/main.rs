//! Stands in for the `claude` CLI in tests, so they're fast, free and deterministic. It speaks the
//! same stream-json protocol, shaped on output captured from the real CLI, and has the behaviors the
//! worker relies on. `crates/agents/tests/claude/contract.rs` holds the real captures, and
//! `tools/smoke/real-claude.sh` runs the whole stack against the real thing.
//!
//! A prompt is a script: `;`-separated commands, each run in order.
//!   say:TEXT        Stream TEXT in chunks, then send the finished message.
//!   slow:N:TEXT     Same, sending N chunks one per 100 ms, so tests can act mid-stream.
//!   burst:N         Same, sending N numbered chunks as fast as possible.
//!   tool:COMMAND    Ask for approval if the mode requires it, then run COMMAND with `sh -c`.
//!   hang:MS         A tool that takes MS milliseconds. Cancelling stops it.
//!   partial:TEXT    Start a message and leave it unfinished, so a crash can cut it off.
//!   holdpipe:SECS   Leave a process holding stdout open that long, so an exit takes time to notice.
//!   recall          Say every prompt this conversation has seen, to prove a resume worked.
//!   crash:CODE      Exit immediately with CODE.
//!   env:NAME        Say the value of an environment variable.
//!   plan:TEXT       Present a plan for approval, which leaves plan mode if it's approved.
//!   ask:Q?=A/B|*R?=C/D  Ask questions with those options. `*` lets a question take several.
//!
//! Conversations persist as files in `$FAKE_CLAUDE_STATE`. `--resume` fails if there isn't one and
//! `--session-id` fails if there is, as with the real CLI.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::needless_pass_by_value,
    reason = "a test program, so it unwraps and takes its values however is shortest"
)]

use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Value, json};

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

struct Config {
    session_id: String,
    conversation: PathBuf,
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

/// Reported the way the real CLI does: in order, and for changes it makes itself.
fn set_mode(out: &Out, mode: &str) {
    *LIVE_MODE.lock() = Some(mode.to_string());
    emit(out, json!({"type":"system","subtype":"status","status":null,"permissionMode":mode}));
}

fn answer_control(out: &Out, request_id: &Value, result: Result<Value, String>) {
    let response = match result {
        Ok(response) => json!({"subtype":"success","request_id":request_id,"response":response}),
        Err(error) => json!({"subtype":"error","request_id":request_id,"error":error}),
    };
    emit(out, json!({"type":"control_response","response":response}));
}

enum Input {
    Prompt(String),
    Approval(Value),
    Interrupt,
    Eof,
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).map(|i| args[i + 1].clone())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let state = PathBuf::from(std::env::var("FAKE_CLAUDE_STATE").expect("FAKE_CLAUDE_STATE"));
    std::fs::create_dir_all(&state).unwrap();
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
    // The real CLI's flag doesn't list `default`, although that's what it calls the mode once running.
    let launch_mode = flag(&args, "--permission-mode");
    if launch_mode.as_deref() == Some("default") {
        eprintln!("error: option '--permission-mode <mode>' argument 'default' is invalid.");
        std::process::exit(1);
    }
    *LIVE_MODE.lock() = launch_mode.map(|mode| if mode == "manual" { "default".to_string() } else { mode });
    *LIVE_MODEL.lock() = flag(&args, "--model");
    let config = Config { conversation: state.join(&session_id), session_id };

    let (input_tx, input_rx) = mpsc::channel();
    let control_out = out.clone();
    std::thread::spawn(move || read_stdin(input_tx, control_out));

    // The real CLI finishes its turn on EOF, then exits. Prompts sent mid-turn fold into that turn.
    let mut queued: VecDeque<String> = VecDeque::new();
    let mut eof = false;
    loop {
        let script = match queued.pop_front() {
            Some(script) => script,
            None if eof => return,
            None => match input_rx.recv() {
                Ok(Input::Prompt(text)) => text,
                Ok(Input::Interrupt | Input::Approval(_)) => continue,
                Ok(Input::Eof) | Err(_) => return,
            },
        };
        run_turn(&config, &out, script, &input_rx, &mut queued, &mut eof);
    }
}

fn read_stdin(input: mpsc::Sender<Input>, out: Out) {
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
        let item = match value["type"].as_str() {
            Some("user") => {
                Input::Prompt(value["message"]["content"][0]["text"].as_str().unwrap_or_default().to_string())
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
                    if model == "not-a-model" {
                        answer_control(&out, &value["request_id"], Err(format!("Model '{model}' not found")));
                    } else {
                        *LIVE_MODEL.lock() = Some(model.to_string());
                        answer_control(&out, &value["request_id"], Ok(json!({"model":model})));
                    }
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
fn pause(rx: &Receiver<Input>, wait: Duration, queued: &mut VecDeque<String>, eof: &mut bool) -> bool {
    match rx.recv_timeout(wait) {
        Ok(Input::Interrupt) => true,
        Ok(Input::Prompt(text)) => {
            queued.push_back(text);
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
