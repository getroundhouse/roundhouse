//! One turn: running a prompt's script, one command at a time.

use std::collections::VecDeque;
use std::os::unix::process::CommandExt as _;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use serde_json::{Value, json};

use crate::tools::{ask_questions, edit_file, present_plan, run_tool};
use crate::{Config, Input, Out, Prompt, current_mode, current_model, emit, lifecycle, pause};
use crate::{advisor, background, cache, fail, mcp, read, subagent};

mod prose;

use prose::prose;

/// What a streamed block holds.
#[derive(Clone, Copy)]
pub(crate) enum Block {
    Text,
    Thinking,
}

impl Block {
    /// Its type, which is also the field its text goes in.
    fn name(self) -> &'static str {
        match self {
            Block::Text => "text",
            Block::Thinking => "thinking",
        }
    }
}

/// How many messages this process has started.
static MESSAGES: AtomicU32 = AtomicU32::new(0);

/// A new message's ID. It's unique within a session, as the real CLI's are, so it counts across turns, and a resumed
/// conversation is another process.
pub(crate) fn message_id() -> String {
    let number = MESSAGES.fetch_add(1, Ordering::Relaxed) + 1;
    format!("msg_fake_{}_{number}", std::process::id())
}

pub(crate) struct Turn<'a> {
    pub(crate) config: &'a Config,
    pub(crate) out: &'a Out,
    /// The call that started the subagent whose messages these are, if they're a subagent's.
    pub(crate) parent: Option<&'a str>,
}

impl Turn<'_> {
    pub(crate) fn stream_event(&self, message_id: &str, event: Value) {
        emit(
            self.out,
            json!({"type":"stream_event","event":event,"session_id":self.config.session_id,
            "parent_tool_use_id":self.parent,"api_message_id":message_id}),
        );
    }

    pub(crate) fn finish_message(&self, message_id: &str, block: Value) {
        emit(
            self.out,
            json!({"type":"assistant","message":{"id":message_id,"model":"fake-model","role":"assistant",
            "content":[block]},"parent_tool_use_id":self.parent,"session_id":self.config.session_id}),
        );
    }

    /// Returns true if interrupted part way.
    pub(crate) fn say(
        &mut self,
        block: Block,
        chunks: Vec<String>,
        gap: Duration,
        rx: &Receiver<Input>,
        queued: &mut VecDeque<Prompt>,
        eof: &mut bool,
    ) -> bool {
        let id = message_id();
        cache::message_start(self, &id);
        let name = block.name();
        self.stream_event(&id, json!({"type":"content_block_start","index":0,"content_block":{"type":name,(name):""}}));
        let mut text = String::new();
        for chunk in chunks {
            let delta = json!({"type":format!("{name}_delta"),(name):chunk});
            self.stream_event(&id, json!({"type":"content_block_delta","index":0,"delta":delta}));
            text.push_str(&chunk);
            if gap > Duration::ZERO && pause(rx, gap, queued, eof) {
                return true;
            }
        }
        // The real CLI sends the finished block before content_block_stop.
        self.finish_message(&id, json!({"type":name,(name):text}));
        self.stream_event(&id, json!({"type":"content_block_stop","index":0}));
        self.stream_event(&id, json!({"type":"message_stop"}));
        false
    }

    /// A call to the tool `name`, rather than a command.
    pub(crate) fn named_tool_use(&mut self, name: &str, input: Value) -> String {
        let id = message_id();
        let tool_use_id = format!("toolu_{id}");
        self.finish_message(&id, json!({"type":"tool_use","id":tool_use_id,"name":name,"input":input}));
        tool_use_id
    }

    pub(crate) fn tool_use(&mut self, command: &str) -> String {
        let id = message_id();
        let tool_use_id = format!("toolu_{id}");
        self.finish_message(&id, json!({"type":"tool_use","id":tool_use_id,"name":"Bash","input":{"command":command}}));
        tool_use_id
    }

    pub(crate) fn tool_result(&self, tool_use_id: &str, content: &str, is_error: bool) {
        emit(self.out, self.result_line(tool_use_id, content, is_error));
    }

    /// The line a tool's result comes in.
    pub(crate) fn result_line(&self, tool_use_id: &str, content: &str, is_error: bool) -> serde_json::Value {
        json!({"type":"user","message":{"role":"user","content":[{"type":"tool_result",
        "tool_use_id":tool_use_id,"content":content,"is_error":is_error}]},"parent_tool_use_id":self.parent,
        "session_id":self.config.session_id})
    }
}

pub(crate) fn state_event(config: &Config, out: &Out, state: &str) {
    emit(out, json!({"type":"system","subtype":"session_state_changed","state":state,"session_id":config.session_id}));
}

/// Adds a prompt to what the conversation has seen, which `recall` says and a resume keeps.
fn remember(config: &Config, prompt: &str) {
    let mut seen = std::fs::read_to_string(&config.conversation).unwrap_or_default();
    seen.push_str(prompt);
    seen.push('\n');
    std::fs::write(&config.conversation, &seen).unwrap();
}

/// What the real CLI writes as a turn begins: that it's ready, and in its first, what's left of the account's limits.
pub(crate) fn begin(config: &Config, out: &Out) {
    emit(
        out,
        json!({"type":"system","subtype":"init","session_id":config.session_id,"model":current_model(),
        "cwd":std::env::current_dir().unwrap(),"permissionMode":current_mode(),"claude_code_version":"fake"}),
    );
    if crate::account::says_limits(&config.conversation) {
        emit(out, crate::account::rate_limit_event(&config.session_id));
    }
}

pub(crate) fn run_turn(
    config: &Config,
    out: &Out,
    first: Prompt,
    by_itself: bool,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<Prompt>,
    eof: &mut bool,
) {
    remember(config, &first.text);
    if !by_itself {
        state_event(config, out, "running");
    }
    lifecycle(out, &first, "started");
    begin(config, out);

    let mut turn = Turn { config, out, parent: None };
    let mut stopped = false;
    let mut pending: VecDeque<String> = VecDeque::from([first.text.clone()]);
    let mut taken = vec![first];
    while let Some(next) = pending.pop_front() {
        for command in next.split(';') {
            stopped = run_command(&mut turn, command.trim(), rx, queued, eof);
            if stopped {
                break;
            }
        }
        // A prompt that arrived mid-turn folds into this turn, with no result of its own.
        if stopped {
            break;
        }
        while let Some(prompt) = queued.pop_front() {
            remember(config, &prompt.text);
            lifecycle(out, &prompt, "started");
            pending.push_back(prompt.text.clone());
            taken.push(prompt);
        }
    }

    if let Some(failed) = fail::ended(&config.session_id) {
        emit(out, failed);
    } else if stopped {
        // Stop means stop, as the worker asks with `cancel_queued`: what hadn't been taken goes unread.
        for prompt in queued.drain(..) {
            lifecycle(out, &prompt, "cancelled");
        }
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
        emit(out, crate::result::success(&config.session_id));
    }
    let ended = if stopped { "cancelled" } else { "completed" };
    for prompt in &taken {
        lifecycle(out, prompt, ended);
    }
    state_event(config, out, "idle");
}

/// Returns true if the turn was interrupted.
pub(crate) fn hang(
    turn: &mut Turn,
    total_ms: u64,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<Prompt>,
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

/// What a command that answers in one piece says, if `command` is one.
fn answer(turn: &Turn, command: &str) -> Option<String> {
    if command == "attached" {
        return Some(crate::attached());
    }
    if command == "effort" {
        return Some(format!("effort={}", crate::current_effort()));
    }
    if command == "cold" {
        cache::expire();
        return Some("the cache had expired".to_string());
    }
    if command == "keep-effort" {
        *crate::KEEPS_EFFORT.lock() = true;
        return Some("keeping my effort".to_string());
    }
    if command == "recall" {
        let seen = std::fs::read_to_string(&turn.config.conversation).unwrap_or_default();
        return Some(format!("I have seen: {}", seen.trim().replace('\n', " | ")));
    }
    let name = command.strip_prefix("env:")?;
    Some(format!("{name}={}", std::env::var(name).unwrap_or_else(|_| "<unset>".to_string())))
}

/// Returns true if the turn goes no further: it was interrupted, or a request to the model failed.
pub(crate) fn run_command(
    turn: &mut Turn,
    command: &str,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<Prompt>,
    eof: &mut bool,
) -> bool {
    if let Some(text) = answer(turn, command) {
        return turn.say(Block::Text, vec![text], Duration::ZERO, rx, queued, eof);
    }
    if let Some(text) = command.strip_prefix("say:") {
        let chunks: Vec<String> = text.chars().collect::<Vec<_>>().chunks(4).map(|c| c.iter().collect()).collect();
        return turn.say(Block::Text, chunks, Duration::ZERO, rx, queued, eof);
    }
    if let Some(rest) = command.strip_prefix("slow:") {
        let (count, text) = rest.split_once(':').unwrap();
        let chunks: Vec<String> = (0..count.parse().unwrap()).map(|i| format!("{text}{i} ")).collect();
        return turn.say(Block::Text, chunks, Duration::from_millis(100), rx, queued, eof);
    }
    if let Some(rest) = command.strip_prefix("think:") {
        let (count, text) = rest.split_once(':').unwrap();
        let chunks: Vec<String> = (0..count.parse().unwrap()).map(|i| format!("{text}{i} ")).collect();
        return turn.say(Block::Thinking, chunks, Duration::from_millis(100), rx, queued, eof);
    }
    if let Some(count) = command.strip_prefix("burst:") {
        let chunks: Vec<String> = (0..count.parse::<u32>().unwrap()).map(|i| format!("{i} ")).collect();
        return turn.say(Block::Text, chunks, Duration::ZERO, rx, queued, eof);
    }
    if let Some(rest) = command.strip_prefix("prose:") {
        let (paragraphs, seed) = rest.split_once(':').unwrap();
        let chunks = prose(paragraphs.parse().unwrap(), seed.parse().unwrap());
        return turn.say(Block::Text, chunks, Duration::ZERO, rx, queued, eof);
    }
    if let Some(rest) = command.strip_prefix("ponder:") {
        let (paragraphs, seed) = rest.split_once(':').unwrap();
        let chunks = prose(paragraphs.parse().unwrap(), seed.parse().unwrap());
        return turn.say(Block::Thinking, chunks, Duration::ZERO, rx, queued, eof);
    }
    if let Some(rest) = command.strip_prefix("late:") {
        return late(turn, rest);
    }
    if let Some(text) = command.strip_prefix("partial:") {
        let id = message_id();
        turn.stream_event(&id, json!({"type":"message_start","message":{"id":id,"role":"assistant"}}));
        turn.stream_event(
            &id,
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}}),
        );
        return false;
    }
    if command == "recall" {
        let seen = std::fs::read_to_string(&turn.config.conversation).unwrap_or_default();
        let text = format!("I have seen: {}", seen.trim().replace('\n', " | "));
        return turn.say(Block::Text, vec![text], Duration::ZERO, rx, queued, eof);
    }
    if let Some(name) = command.strip_prefix("env:") {
        let text = format!("{name}={}", std::env::var(name).unwrap_or_else(|_| "<unset>".to_string()));
        return turn.say(Block::Text, vec![text], Duration::ZERO, rx, queued, eof);
    }
    act(turn, command, rx, queued, eof)
}

/// The commands that do something rather than write: calls, questions, plans, subagents, work in the background,
/// and ways to misbehave. Returns true if the turn goes no further, as [`run_command`] does.
fn act(turn: &mut Turn, command: &str, rx: &Receiver<Input>, queued: &mut VecDeque<Prompt>, eof: &mut bool) -> bool {
    if let Some(command) = command.strip_prefix("tool:") {
        return run_tool(turn, command, rx, queued, eof);
    }
    if let Some(size) = command.strip_prefix("edit:") {
        return edit_file(turn, size);
    }
    if let Some(ms) = command.strip_prefix("hang:") {
        return hang(turn, ms.parse().unwrap(), rx, queued, eof);
    }
    if let Some(error) = command.strip_prefix("fail:") {
        return fail::request(turn, error);
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
    if let Some(asked) = subagent::Asked::read(command) {
        return subagent::start(turn, &asked, rx, queued, eof);
    }
    if let Some((call, file)) = background::Call::read(command) {
        return background::start(turn, call, file);
    }
    if let Some(file) = command.strip_prefix("read:") {
        read::image(turn, file);
    }
    if let Some(tool) = command.strip_prefix("mcp:") {
        mcp::call(turn, tool);
    }
    if let Some(reply) = advisor::Reply::read(command) {
        advisor::ask(turn, &reply);
    }
    false
}

/// Never interrupted, since what it leaves behind is the point.
fn late(turn: &Turn, rest: &str) -> bool {
    let (file, text) = rest.rsplit_once(':').unwrap();
    let delta = json!({"type":"stream_event","session_id":turn.config.session_id,"parent_tool_use_id":null,
        "api_message_id":"msg_late",
        "event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}}});
    outlive_until(file, &[delta, json!({"type":"late_marker"})]);
    false
}

/// Leaves a process holding stdout open until `file` exists, and then writing `lines` to it. It's in its
/// own process group, as the real CLI's tools are, so a signal to this one's group can't stop it. The
/// worker signals the group when a prompt reaches an agent that has died but isn't reaped yet.
fn outlive_until(file: &str, lines: &[Value]) {
    let lines: Vec<String> = lines.iter().map(Value::to_string).collect();
    let script = r#"while [ ! -e "$1" ]; do sleep 0.05; done; shift; for line in "$@"; do printf '%s\n' "$line"; done"#;
    #[expect(clippy::zombie_processes, reason = "never waited on, because the point is that it outlives this process")]
    std::process::Command::new("sh")
        .args(["-c", script, "sh", file])
        .args(lines)
        .process_group(0)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
}
