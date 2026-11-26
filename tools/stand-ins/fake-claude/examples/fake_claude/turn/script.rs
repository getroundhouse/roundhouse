use super::prose;
use super::writing::{Block, Turn, message_id};
use crate::tools::{ask_questions, edit_file, present_plan, run_tool};
use crate::waiting::{hang, pause_until_made, until};
use crate::{
    Config, Input, Out, Prompt, advisor, asking_subagents, background, cache, current_mode, current_model, emit, fail,
    lifecycle, mcp, read, subagent,
};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// How often `slow-until` and `think-until` say another chunk while they look for their file. Only a pace, so they
/// don't say a great deal while a test catches up: a test waits for `FILE.said`, never for a number of chunks.
const UNTIL_POLL: Duration = Duration::from_millis(100);

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
    let mut init = json!({"type":"system","subtype":"init","session_id":config.session_id,"model":current_model(),
        "cwd":std::env::current_dir().unwrap(),"permissionMode":current_mode(),"claude_code_version":"fake"});
    if let Some((servers, tools)) = crate::mcp::listed() {
        init["mcp_servers"] = servers.into();
        init["tools"] = tools.into();
    }
    emit(out, init);
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

    let mut turn = Turn { config, out, within: None };
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
    if let Some(text) = command.strip_prefix("whole:") {
        return turn.say(Block::Text, [text.to_string()], Duration::ZERO, rx, queued, eof);
    }
    if let Some(rest) = command.strip_prefix("slow-until:") {
        return turn.say(Block::Text, until(rest), UNTIL_POLL, rx, queued, eof);
    }
    if let Some(rest) = command.strip_prefix("think-until:") {
        return turn.say(Block::Thinking, until(rest), UNTIL_POLL, rx, queued, eof);
    }
    if let Some(count) = command.strip_prefix("burst:") {
        let chunks = Vec::from_fn(count.parse().unwrap(), |i| format!("{i} "));
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
    if command == "hang" {
        return hang(turn, None, rx, queued, eof);
    }
    if let Some(file) = command.strip_prefix("hang-until:") {
        return hang(turn, Some(Path::new(file)), rx, queued, eof);
    }
    if let Some(file) = command.strip_prefix("request-until:") {
        return pause_until_made(Path::new(file), rx, queued, eof);
    }
    if let Some(file) = command.strip_prefix("retry-until:") {
        fail::retrying(turn);
        return pause_until_made(Path::new(file), rx, queued, eof);
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
    if let Some(spec) = command.strip_prefix("agents-ask:") {
        return asking_subagents::start(turn, spec, rx, queued, eof);
    }
    if let Some(asked) = subagent::Asked::read(command) {
        return subagent::start(turn, &asked, rx, queued, eof);
    }
    if let Some((call, file)) = background::Call::read(command) {
        return background::start(turn, call, file);
    }
    if let Some(file) = command.strip_prefix("read:") {
        read::image(turn, file);
    } else if let Some(tool) = command.strip_prefix("mcp:") {
        mcp::call(turn, tool);
    } else if let Some(reply) = advisor::Reply::read(command) {
        advisor::ask(turn, &reply);
    } else {
        refuse_if_a_command(command);
    }
    false
}

/// Ends the agent, naming the command, if `command` names one it doesn't have, as `NAME:` does, so a script that asks
/// for what it doesn't do, such as a command it no longer has, fails the test rather than passing without it. Anything
/// else is words, such as a prompt a person might type, which it takes in and answers with nothing.
fn refuse_if_a_command(command: &str) {
    let Some((name, _)) = command.split_once(':') else { return };
    if !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
        eprintln!("fake_claude: there's no command `{name}:`, as in `{command}`");
        std::process::exit(2);
    }
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
