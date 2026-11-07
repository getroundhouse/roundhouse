//! One turn: running a prompt's script, one command at a time.

use std::collections::VecDeque;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use serde_json::{Value, json};

use crate::tools::{ask_questions, present_plan, run_tool};
use crate::{Config, Input, Out, current_mode, current_model, emit, pause};

pub(crate) struct Turn<'a> {
    pub(crate) config: &'a Config,
    pub(crate) out: &'a Out,
    pub(crate) next_message: u32,
}

impl Turn<'_> {
    pub(crate) fn message_id(&mut self) -> String {
        self.next_message += 1;
        format!("msg_fake_{}_{}", std::process::id(), self.next_message)
    }

    pub(crate) fn stream_event(&self, message_id: &str, event: Value) {
        emit(
            self.out,
            json!({"type":"stream_event","event":event,"session_id":self.config.session_id,
            "parent_tool_use_id":null,"api_message_id":message_id}),
        );
    }

    pub(crate) fn finish_message(&self, message_id: &str, block: Value) {
        emit(
            self.out,
            json!({"type":"assistant","message":{"id":message_id,"model":"fake-model","role":"assistant",
            "content":[block]},"parent_tool_use_id":null,"session_id":self.config.session_id}),
        );
    }

    /// Returns true if interrupted part way.
    pub(crate) fn say(
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
    pub(crate) fn named_tool_use(&mut self, name: &str) -> String {
        let id = self.message_id();
        let tool_use_id = format!("toolu_{id}");
        self.finish_message(&id, json!({"type":"tool_use","id":tool_use_id,"name":name,"input":{}}));
        tool_use_id
    }

    pub(crate) fn tool_use(&mut self, command: &str) -> String {
        let id = self.message_id();
        let tool_use_id = format!("toolu_{id}");
        self.finish_message(&id, json!({"type":"tool_use","id":tool_use_id,"name":"Bash","input":{"command":command}}));
        tool_use_id
    }

    pub(crate) fn tool_result(&self, tool_use_id: &str, content: &str, is_error: bool) {
        emit(
            self.out,
            json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result",
            "tool_use_id":tool_use_id,"content":content,"is_error":is_error}]},"parent_tool_use_id":null,
            "session_id":self.config.session_id}),
        );
    }
}

pub(crate) fn state_event(config: &Config, out: &Out, state: &str) {
    emit(out, json!({"type":"system","subtype":"session_state_changed","state":state,"session_id":config.session_id}));
}

pub(crate) fn run_turn(
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
pub(crate) fn hang(
    turn: &mut Turn,
    total_ms: u64,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<String>,
    eof: &mut bool,
) -> bool {
    let tool_use_id = turn.tool_use("hang");
    let mut waited = 0;
    while waited < total_ms {
        if pause(rx, Duration::from_millis(50), queued, eof) {
            turn.tool_result(&tool_use_id, "stopped", true);
            return true;
        }
        waited += 50;
    }
    turn.tool_result(&tool_use_id, "finished hanging", false);
    false
}

/// Returns true if the turn was interrupted.
pub(crate) fn run_command(
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
        #[expect(
            clippy::zombie_processes,
            reason = "never waited on, because the point is that it outlives this process"
        )]
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
        return hang(turn, ms.parse().unwrap(), rx, queued, eof);
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
