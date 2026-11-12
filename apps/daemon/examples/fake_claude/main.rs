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
//!   ponder:N:SEED   Think N paragraphs of the same made-up prose, as fast as possible.
//!   tool:COMMAND    Ask for approval unless in full access, as the real CLI does in plan mode for a call that
//!                   writes, then run COMMAND with `sh -c`.
//!   edit:N          Edit `notes.txt`, changing each of N lines, without asking or touching the disk. The result
//!                   comes back without a patch, so the edit is drawn from what it replaced.
//!   edit:N:WIDTH    Same, with each line at least WIDTH characters long.
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
//!   plan:TEXT       Present a plan for approval, which leaves plan mode if it's approved, for the mode it was
//!                   entered from. A launch straight into plan mode leaves it for `default`, as the real CLI's does.
//!   ask:Q?=A/B|*R?=C/D  Ask questions with those options. `*` lets a question take several.
//!   attached        Say what the newest prompt carried besides its script: each image's type and size, each
//!                   file given in the prompt by its name and first line, and each file given by its path by its
//!                   name and size on disk. The script is the prompt's last text block.
//!   background:FILE Start a background task, and go on without it. Once FILE exists it finishes, and the agent
//!                   picks up again by itself to say `finished FILE`, in a turn that starts with `init` alone.
//!                   SIGTERM stops it, and lists none left.
//!   monitor:FILE    The same, started by a `Monitor` call, so it's a monitor.
//!   agent:TEXT      Start a subagent for TEXT in the background, which makes a call and reports a second later.
//!   agent-foreground:TEXT  The same in the foreground, so the call's result waits for its report.
//!   agent-calls:N:TEXT  The same in the background, making N calls, so its work is long enough to scroll.
//!   agent-until:FILE:TEXT  The same in the background, working until FILE exists, so a test can catch it at work.
//!   read:FILE       Read FILE, a PNG, as the real CLI reads an image: as an image block in the result, again in its
//!                   record of the read, and then a line for the model saying how it shrank it.
//!   advise          Ask the advisor, as the real CLI does, and get back a reply encrypted for the model alone.
//!   advise:TEXT     Same, with TEXT as a reply that can be read.
//!   advise-failed:CODE  Same, failing with the error CODE, such as `overloaded`.
//!   advise-unanswered   Same, with no reply at all, as the real CLI's call after one of the agent's own tools gets.
//!
//! A prompt of `/compact`, with what to keep after it or not, is no script: it compacts, as the real CLI does, in a turn
//! of its own.
//!
//! A prompt sent mid-turn folds in once the script running finishes, and a prompt with a `uuid` is reported with
//! `command_lifecycle` lines, as the real CLI does. Stopping a turn drops the prompts it hadn't taken, as the worker asks
//! with `cancel_queued`.
//!
//! Conversations persist as files in `$FAKE_CLAUDE_STATE`. `--resume` fails if there isn't one and
//! `--session-id` fails if there is, as with the real CLI.
//!
//! With `--no-session-persistence`, it's the one-off call that titles a prompt instead. See `title.rs`. And as
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
mod cache;
mod compact;
mod prompt;
mod read;
mod result;
mod settings;
mod subagent;
mod title;
mod tools;
mod turn;

use prompt::{attached, read_prompt};
pub(crate) use settings::{BEFORE_PLAN, KEEPS_EFFORT, current_effort, current_mode, current_model, set_mode};
use settings::{apply_flag_settings, flag, set_model, start_as_launched};
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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let state = std::env::var("FAKE_CLAUDE_STATE").map(PathBuf::from);
    if args == ["auth", "status", "--json"] {
        println!("{}", account::auth_status(state.as_deref().ok()));
        return;
    }
    let state = state.expect("FAKE_CLAUDE_STATE");
    std::fs::create_dir_all(&state).unwrap();
    if flag(&args, "--output-format").as_deref() == Some("stream-json") && !args.iter().any(|a| a == "--verbose") {
        eprintln!("Error: When using --print, --output-format=stream-json requires --verbose");
        std::process::exit(1);
    }
    // A session is saved, so it can be resumed, and only a one-off call isn't.
    if args.iter().any(|a| a == "--no-session-persistence") {
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
        if !by_itself && compact::if_asked(&config, &out, &prompt) {
            continue;
        }
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
