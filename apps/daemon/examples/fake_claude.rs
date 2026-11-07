//! Stands in for the `claude` CLI in tests, so they're fast, free and deterministic. It speaks the
//! same stream-json protocol, shaped on output captured from the real CLI, and has the behaviors the
//! worker relies on. `crates/agents/tests/claude_contract.rs` holds the real captures, and
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

use std::collections::VecDeque;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

type Out = Arc<Mutex<std::io::Stdout>>;

/// Ignores write errors: an orphan keeps working after whoever read its output has gone.
fn emit(out: &Out, value: Value) {
    let mut out = out.lock().unwrap();
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
    LIVE_MODE.lock().unwrap().clone().unwrap_or_else(|| "default".to_string())
}

fn current_model() -> String {
    LIVE_MODEL.lock().unwrap().clone().unwrap_or_else(|| "fake-model".to_string())
}

/// Reported the way the real CLI does: in order, and for changes it makes itself.
fn set_mode(out: &Out, mode: &str) {
    *LIVE_MODE.lock().unwrap() = Some(mode.to_string());
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
    *LIVE_MODE.lock().unwrap() = launch_mode.map(|mode| if mode == "manual" { "default".to_string() } else { mode });
    *LIVE_MODEL.lock().unwrap() = flag(&args, "--model");
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
                        *LIVE_MODEL.lock().unwrap() = Some(model.to_string());
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

struct Turn<'a> {
    config: &'a Config,
    out: &'a Out,
    next_message: u32,
}

impl Turn<'_> {
    fn message_id(&mut self) -> String {
        self.next_message += 1;
        format!("msg_fake_{}_{}", std::process::id(), self.next_message)
    }

    fn stream_event(&self, message_id: &str, event: Value) {
        emit(
            self.out,
            json!({"type":"stream_event","event":event,"session_id":self.config.session_id,
            "parent_tool_use_id":null,"api_message_id":message_id}),
        );
    }

    fn finish_message(&self, message_id: &str, block: Value) {
        emit(
            self.out,
            json!({"type":"assistant","message":{"id":message_id,"model":"fake-model","role":"assistant",
            "content":[block]},"parent_tool_use_id":null,"session_id":self.config.session_id}),
        );
    }

    /// Returns true if interrupted part way.
    fn say(
        &mut self,
        chunks: Vec<String>,
        gap: Duration,
        rx: &Receiver<Input>,
        queued: &mut VecDeque<String>,
        eof: &mut bool,
    ) -> bool {
        let id = self.message_id();
        self.stream_event(&id, json!({"type":"message_start","message":{"id":id,"role":"assistant"}}));
        self.stream_event(
            &id,
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        );
        let mut text = String::new();
        for chunk in chunks {
            self.stream_event(
                &id,
                json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":chunk}}),
            );
            text.push_str(&chunk);
            if gap > Duration::ZERO && pause(rx, gap, queued, eof) {
                return true;
            }
        }
        // The real CLI sends the finished block before content_block_stop.
        self.finish_message(&id, json!({"type":"text","text":text}));
        self.stream_event(&id, json!({"type":"content_block_stop","index":0}));
        self.stream_event(&id, json!({"type":"message_stop"}));
        false
    }

    /// A tool that's its own card in the app, so it goes by its real name.
    fn named_tool_use(&mut self, name: &str) -> String {
        let id = self.message_id();
        let tool_use_id = format!("toolu_{id}");
        self.finish_message(&id, json!({"type":"tool_use","id":tool_use_id,"name":name,"input":{}}));
        tool_use_id
    }

    fn tool_use(&mut self, command: &str) -> String {
        let id = self.message_id();
        let tool_use_id = format!("toolu_{id}");
        self.finish_message(&id, json!({"type":"tool_use","id":tool_use_id,"name":"Bash","input":{"command":command}}));
        tool_use_id
    }

    fn tool_result(&self, tool_use_id: &str, content: &str, is_error: bool) {
        emit(
            self.out,
            json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result",
            "tool_use_id":tool_use_id,"content":content,"is_error":is_error}]},"parent_tool_use_id":null,
            "session_id":self.config.session_id}),
        );
    }
}

fn state_event(config: &Config, out: &Out, state: &str) {
    emit(out, json!({"type":"system","subtype":"session_state_changed","state":state,"session_id":config.session_id}));
}

fn run_turn(
    config: &Config,
    out: &Out,
    script: String,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<String>,
    eof: &mut bool,
) {
    let mut seen = std::fs::read_to_string(&config.conversation).unwrap_or_default();
    seen.push_str(&script);
    seen.push('\n');
    std::fs::write(&config.conversation, &seen).unwrap();

    state_event(config, out, "running");
    emit(
        out,
        json!({"type":"system","subtype":"init","session_id":config.session_id,"model":current_model(),
        "cwd":std::env::current_dir().unwrap(),"permissionMode":current_mode(),"claude_code_version":"fake"}),
    );

    let mut turn = Turn { config, out, next_message: 0 };
    let mut interrupted = false;
    let mut pending: VecDeque<String> = VecDeque::from([script]);
    while let Some(next) = pending.pop_front() {
        for command in next.split(';') {
            interrupted = run_command(&mut turn, command.trim(), rx, queued, eof);
            if interrupted {
                break;
            }
        }
        // A prompt that arrived mid-turn folds into this turn, with no result of its own.
        if interrupted {
            break;
        }
        while let Some(prompt) = queued.pop_front() {
            let mut seen = std::fs::read_to_string(&config.conversation).unwrap_or_default();
            seen.push_str(&prompt);
            seen.push('\n');
            std::fs::write(&config.conversation, &seen).unwrap();
            pending.push_back(prompt);
        }
    }

    if interrupted {
        emit(
            out,
            json!({"type":"user","message":{"role":"user","content":[{"type":"text",
            "text":"[Request interrupted by user]"}]},"parent_tool_use_id":null,"session_id":config.session_id}),
        );
        emit(
            out,
            json!({"type":"result","subtype":"error_during_execution","is_error":true,
            "terminal_reason":"aborted_tools","duration_ms":1,"num_turns":1,"total_cost_usd":0.0,
            "errors":["[ede_diagnostic]"],"session_id":config.session_id}),
        );
    } else {
        emit(
            out,
            json!({"type":"result","subtype":"success","is_error":false,"terminal_reason":"completed",
            "result":"done","duration_ms":1,"num_turns":1,"total_cost_usd":0.0,"session_id":config.session_id}),
        );
    }
    state_event(config, out, "idle");
}

/// Returns true if the turn was interrupted.
fn run_command(
    turn: &mut Turn,
    command: &str,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<String>,
    eof: &mut bool,
) -> bool {
    if let Some(text) = command.strip_prefix("say:") {
        let chunks: Vec<String> = text.chars().collect::<Vec<_>>().chunks(4).map(|c| c.iter().collect()).collect();
        return turn.say(chunks, Duration::ZERO, rx, queued, eof);
    }
    if let Some(rest) = command.strip_prefix("slow:") {
        let (count, text) = rest.split_once(':').unwrap();
        let chunks: Vec<String> = (0..count.parse().unwrap()).map(|i| format!("{text}{i} ")).collect();
        return turn.say(chunks, Duration::from_millis(100), rx, queued, eof);
    }
    if let Some(count) = command.strip_prefix("burst:") {
        let chunks: Vec<String> = (0..count.parse::<u32>().unwrap()).map(|i| format!("{i} ")).collect();
        return turn.say(chunks, Duration::ZERO, rx, queued, eof);
    }
    if let Some(seconds) = command.strip_prefix("holdpipe:") {
        // Never waited on, because the point is that it outlives this process.
        #[expect(clippy::zombie_processes)]
        std::process::Command::new("sleep")
            .arg(seconds)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        return false;
    }
    if let Some(text) = command.strip_prefix("partial:") {
        let id = turn.message_id();
        turn.stream_event(&id, json!({"type":"message_start","message":{"id":id,"role":"assistant"}}));
        turn.stream_event(
            &id,
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}}),
        );
        return false;
    }
    if let Some(command) = command.strip_prefix("tool:") {
        return run_tool(turn, command, rx, queued, eof);
    }
    if let Some(ms) = command.strip_prefix("hang:") {
        let tool_use_id = turn.tool_use("hang");
        let total: u64 = ms.parse().unwrap();
        let mut waited = 0;
        while waited < total {
            if pause(rx, Duration::from_millis(50), queued, eof) {
                turn.tool_result(&tool_use_id, "stopped", true);
                return true;
            }
            waited += 50;
        }
        turn.tool_result(&tool_use_id, "finished hanging", false);
        return false;
    }
    if command == "recall" {
        let seen = std::fs::read_to_string(&turn.config.conversation).unwrap_or_default();
        let text = format!("I have seen: {}", seen.trim().replace('\n', " | "));
        return turn.say(vec![text], Duration::ZERO, rx, queued, eof);
    }
    if let Some(code) = command.strip_prefix("crash:") {
        std::process::exit(code.parse().unwrap());
    }
    if let Some(plan) = command.strip_prefix("plan:") {
        return present_plan(turn, plan, rx, queued, eof);
    }
    if let Some(questions) = command.strip_prefix("ask:") {
        return ask_questions(turn, questions, rx, queued, eof);
    }
    if let Some(name) = command.strip_prefix("env:") {
        let text = format!("{name}={}", std::env::var(name).unwrap_or_else(|_| "<unset>".to_string()));
        return turn.say(vec![text], Duration::ZERO, rx, queued, eof);
    }
    false
}

/// The result reads like the real CLI's, which goes by whether it was given answers.
fn ask_questions(
    turn: &mut Turn,
    spec: &str,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<String>,
    eof: &mut bool,
) -> bool {
    let questions: Vec<Value> = spec
        .split('|')
        .map(|one| {
            let (text, options) = one.split_once('=').unwrap();
            let (multi, text) = text.strip_prefix('*').map_or((false, text), |rest| (true, rest));
            let options: Vec<Value> =
                options.split('/').map(|o| json!({"label":o,"description":format!("Choose {o}")})).collect();
            json!({"question":text,"header":"Question","options":options,"multiSelect":multi})
        })
        .collect();
    let tool_use_id = turn.named_tool_use("AskUserQuestion");
    let request_id = format!("req_{tool_use_id}");
    emit(
        turn.out,
        json!({"type":"control_request","request_id":request_id,"request":{"subtype":"can_use_tool",
        "tool_name":"AskUserQuestion","input":{"questions":questions},"tool_use_id":tool_use_id,
        "permission_suggestions":[]}}),
    );
    state_event(turn.config, turn.out, "requires_action");
    loop {
        match rx.recv() {
            Ok(Input::Approval(response)) => {
                state_event(turn.config, turn.out, "running");
                if response["response"]["behavior"] == "deny" {
                    let message = response["response"]["message"].as_str().unwrap_or("denied");
                    turn.tool_result(&tool_use_id, message, true);
                    return false;
                }
                let answers = response["response"]["updatedInput"]["answers"].as_object();
                let text = match answers {
                    Some(answers) if !answers.is_empty() => {
                        let said: Vec<String> =
                            answers.iter().map(|(q, a)| format!("\"{q}\"={}", a.as_str().unwrap())).collect();
                        format!("Your questions have been answered: {}.", said.join(", "))
                    }
                    _ => "The user did not answer the questions.".to_string(),
                };
                turn.tool_result(&tool_use_id, &text, false);
                return false;
            }
            Ok(Input::Interrupt) => {
                emit(turn.out, json!({"type":"control_cancel_request","request_id":request_id}));
                turn.tool_result(&tool_use_id, "cancelled", true);
                return true;
            }
            Ok(Input::Prompt(text)) => queued.push_back(text),
            Ok(Input::Eof) | Err(_) => {
                *eof = true;
                return true;
            }
        }
    }
}

/// Approving a plan leaves plan mode, and the CLI says so itself.
fn present_plan(
    turn: &mut Turn,
    plan: &str,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<String>,
    eof: &mut bool,
) -> bool {
    let tool_use_id = turn.named_tool_use("ExitPlanMode");
    let request_id = format!("req_{tool_use_id}");
    emit(
        turn.out,
        json!({"type":"control_request","request_id":request_id,"request":{"subtype":"can_use_tool",
        "tool_name":"ExitPlanMode","input":{"plan":plan,"planFilePath":"/tmp/fake-plan.md"},
        "tool_use_id":tool_use_id,"permission_suggestions":[]}}),
    );
    state_event(turn.config, turn.out, "requires_action");
    loop {
        match rx.recv() {
            Ok(Input::Approval(response)) => {
                state_event(turn.config, turn.out, "running");
                if response["response"]["behavior"] == "deny" {
                    let message = response["response"]["message"].as_str().unwrap_or("denied");
                    turn.tool_result(&tool_use_id, message, true);
                    return false;
                }
                set_mode(turn.out, "default");
                turn.tool_result(&tool_use_id, "User has approved your plan.", false);
                return false;
            }
            Ok(Input::Interrupt) => {
                emit(turn.out, json!({"type":"control_cancel_request","request_id":request_id}));
                turn.tool_result(&tool_use_id, "cancelled", true);
                return true;
            }
            Ok(Input::Prompt(text)) => queued.push_back(text),
            Ok(Input::Eof) | Err(_) => {
                *eof = true;
                return true;
            }
        }
    }
}

fn run_tool(
    turn: &mut Turn,
    command: &str,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<String>,
    eof: &mut bool,
) -> bool {
    let tool_use_id = turn.tool_use(command);
    if current_mode() != "bypassPermissions" {
        let request_id = format!("req_{tool_use_id}");
        emit(
            turn.out,
            json!({"type":"control_request","request_id":request_id,"request":{"subtype":"can_use_tool",
            "tool_name":"Bash","input":{"command":command},"description":command,"tool_use_id":tool_use_id,
            "permission_suggestions":[]}}),
        );
        state_event(turn.config, turn.out, "requires_action");
        loop {
            match rx.recv() {
                Ok(Input::Approval(response)) => {
                    state_event(turn.config, turn.out, "running");
                    assert_eq!(response["request_id"], request_id.as_str());
                    if response["response"]["behavior"] == "deny" {
                        let message = response["response"]["message"].as_str().unwrap_or("denied");
                        turn.tool_result(&tool_use_id, message, true);
                        return false;
                    }
                    // The CLI runs what it's given back, which is the input as approved.
                    let approved =
                        response["response"]["updatedInput"]["command"].as_str().unwrap_or(command).to_string();
                    return execute(turn, &tool_use_id, &approved);
                }
                Ok(Input::Interrupt) => {
                    emit(turn.out, json!({"type":"control_cancel_request","request_id":request_id}));
                    turn.tool_result(&tool_use_id, "cancelled", true);
                    return true;
                }
                Ok(Input::Prompt(text)) => queued.push_back(text),
                Ok(Input::Eof) | Err(_) => {
                    *eof = true;
                    return true;
                }
            }
        }
    }
    execute(turn, &tool_use_id, command)
}

fn execute(turn: &mut Turn, tool_use_id: &str, command: &str) -> bool {
    let output = std::process::Command::new("sh").args(["-c", command]).output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    turn.tool_result(tool_use_id, &text, !output.status.success());
    false
}
