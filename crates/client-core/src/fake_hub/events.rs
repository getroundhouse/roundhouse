//! Building the events a world's chats are made of, as the hub would record them from Claude, whose adapter types its
//! calls and what came back for each.

use protocol::attachment::Attachment;
use protocol::capability::calls::call::{
    Command, Edit, LineRange, Monitor, Read, Replace, Replaces, Runs, StartSubagent, Write,
};
use protocol::capability::calls::output::{Ended, FileOutcome, Lines, McpContent};
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::{
    AgentTask, ContentBlock, EventBody, FileChange, LinkedImage, TaskKind, TaskOutcome, TokenUsage, TurnOutcome,
};
use protocol::fixtures;
use protocol::model::{MessageId, ModelName, Seq, SubagentId, ToolUseId};
use protocol::prompt::Prompt;
use protocol::prompt::PromptFrom;
use serde_json::Value;

pub fn user_prompt(text: &str) -> EventBody {
    EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new(text), from: PromptFrom::Person }
}

/// A chat's events from its first turn: `prompt`, such as [`user_prompt`] makes, the agent taking it, and `then`, which
/// is what the agent did and, once the turn has ended, its end. A later turn is added as [`turn`] adds one.
pub fn first_turn(prompt: EventBody, then: impl IntoIterator<Item = EventBody>) -> Vec<EventBody> {
    let mut events = Vec::new();
    start_turn(&mut events, prompt);
    events.extend(then);
    events
}

/// A turn added to a chat's `events`, as the hub records one: `prompt`, such as [`user_prompt`] makes, the agent taking
/// it, what the agent `did`, and the turn's end, as [`finished_turn`] makes one that took three seconds and a cent.
/// Returns where the prompt is.
pub fn turn(events: &mut Vec<EventBody>, prompt: EventBody, did: impl IntoIterator<Item = EventBody>) -> Seq {
    turn_ending_with(events, prompt, did, finished_turn(3, 0.01))
}

/// A turn added to a chat's `events`, as [`turn`] adds one, that ends with `end`, such as [`replied`] makes.
pub fn turn_ending_with(
    events: &mut Vec<EventBody>,
    prompt: EventBody,
    did: impl IntoIterator<Item = EventBody>,
    end: EventBody,
) -> Seq {
    let asked = start_turn(events, prompt);
    events.extend(did);
    events.push(end);
    asked
}

/// The start of a turn added to a chat's `events`: `asked`, which is a prompt, such as [`user_prompt`] makes, or an ask
/// to compact, and the agent taking it. Returns where `asked` is.
pub fn start_turn(events: &mut Vec<EventBody>, asked: EventBody) -> Seq {
    assert!(
        matches!(asked, EventBody::UserPrompt { .. } | EventBody::CompactionAsked { .. }),
        "a turn starts with a prompt or an ask to compact, not {asked:?}",
    );
    let at = next_seq(events);
    events.extend([asked, taken(at)]);
    at
}

/// The `seq` of the next event added to a chat's `events`, since a chat's first is at 1.
pub fn next_seq(events: &[EventBody]) -> Seq {
    Seq::new(u64::try_from(events.len()).unwrap() + 1)
}

/// The agent taking the prompt recorded at `prompt`, which is the first event of every chat here.
pub fn taken(prompt: Seq) -> EventBody {
    EventBody::PromptTaken { prompt }
}

/// Something the agent wrote, as its own message, the way Claude sends each block.
pub fn text(message: &str, text: &str) -> EventBody {
    text_pointing(message, text, Vec::new())
}

/// The same, with the images in the worktree it points to, which the hub kept.
pub fn text_pointing(message: &str, text: &str, images: Vec<LinkedImage>) -> EventBody {
    from_the_agent(message, ContentBlock::Text { text: text.to_string(), images })
}

/// The typed call `id`, in the agent's message that makes it.
pub fn typed_call(message: &str, id: &str, call: Call) -> EventBody {
    from_the_agent(message, fixtures::typed_call(id, call))
}

/// What came back for the typed call `id`.
pub fn typed_result(id: &str, output: CallOutput, is_error: bool) -> EventBody {
    sent_back(fixtures::typed_result(id, output, is_error))
}

/// Running `command`, which the agent said is for `described`, if it said, as the call `id`.
pub fn command(message: &str, id: &str, command: &str, described: Option<&str>) -> EventBody {
    let command = Command { description: described.map(str::to_string), ..fixtures::typed_command(command) };
    typed_call(message, id, Call::Command(command))
}

/// Running `command` in the background, which the agent said is for `described`, as the call `id`.
pub fn in_background(message: &str, id: &str, command: &str, described: &str) -> EventBody {
    let command = Command {
        description: Some(described.to_string()),
        runs: Runs::Background,
        ..fixtures::typed_command(command)
    };
    typed_call(message, id, Call::Command(command))
}

/// Watching what `command` prints, for `described`, as the call `id`, for as long as `timeout_ms`, if it says.
pub fn monitor(message: &str, id: &str, command: &str, described: &str, timeout_ms: Option<u64>) -> EventBody {
    let monitor = Monitor { command: command.into(), description: Some(described.into()), timeout_ms };
    typed_call(message, id, Call::Monitor(monitor))
}

/// Handing `prompt` to a subagent of the persona `persona`, for `described`, as the call `id`, which waits for it to
/// end unless it `runs` in the background.
pub fn subagent(message: &str, id: &str, persona: &str, described: &str, prompt: &str, runs: Runs) -> EventBody {
    let start = StartSubagent {
        persona: Some(persona.into()),
        description: Some(described.into()),
        runs,
        ..fixtures::typed_subagent(prompt)
    };
    typed_call(message, id, Call::Subagent(start))
}

/// The tool `tool` of the MCP server `server`, given `input`, as the call `id`.
pub fn mcp(message: &str, id: &str, server: &str, tool: &str, input: Value) -> EventBody {
    typed_call(message, id, Call::Mcp { server: server.into(), tool: tool.into(), input })
}

/// Reading all of the file at `path`, as the call `id`.
pub fn read(message: &str, id: &str, path: &str) -> EventBody {
    typed_call(message, id, Call::Read(Read { path: path.into(), lines: None, pages: None }))
}

/// Reading `count` lines of the file at `path`, from line `first`, as the call `id`.
pub fn read_lines(message: &str, id: &str, path: &str, first: u32, count: u32) -> EventBody {
    let lines = Some(LineRange { first, count: Some(count) });
    typed_call(message, id, Call::Read(Read { path: path.into(), lines, pages: None }))
}

/// Writing the file at `path` whole, as the call `id`.
pub fn write(message: &str, id: &str, path: &str, content: &str) -> EventBody {
    typed_call(message, id, Call::Write(Write { path: path.into(), content: content.into() }))
}

/// Replacing `old` with `new` once in the file at `path`, as the call `id`.
pub fn edit(message: &str, id: &str, path: &str, old: &str, new: &str) -> EventBody {
    let replace = Replace { old: old.into(), new: new.into(), replaces: Replaces::One };
    typed_call(message, id, Call::Edit(Edit { path: path.into(), edits: vec![replace] }))
}

/// What the agent thought, as its own message.
pub fn thought(message: &str, text: &str) -> EventBody {
    from_the_agent(message, ContentBlock::Thinking { text: text.to_string() })
}

/// What the advisor said. Its reply comes from the agent, in the call's message, since the API runs it.
pub fn advised(message: &str, id: &str, text: &str) -> EventBody {
    from_the_agent(message, fixtures::typed_result(id, words(text), false))
}

/// The advisor's reply, encrypted for the model alone.
pub fn advised_encrypted(message: &str, id: &str) -> EventBody {
    from_the_agent(message, fixtures::typed_result(id, CallOutput::Encrypted, false))
}

/// Why asking the advisor failed.
pub fn advice_failed(message: &str, id: &str, text: &str) -> EventBody {
    from_the_agent(message, fixtures::typed_result(id, words(text), true))
}

fn from_the_agent(message: &str, block: ContentBlock) -> EventBody {
    EventBody::Assistant {
        message_id: Some(MessageId::from(message)),
        subagent: None,
        model: Some("claude-opus-5-5".to_string()),
        blocks: vec![block],
        first_index: None,
    }
}

/// What a command printed, `text`, as what came back for the call `id`, which said nothing of how it exited.
pub fn printed(id: &str, text: &str) -> EventBody {
    typed_result(id, fixtures::command_output(text, None, Ended::Exited), false)
}

/// What a command printed, `text`, as it failed, exiting with `code`, as what came back for the call `id`.
pub fn exited(id: &str, text: &str, code: i32) -> EventBody {
    typed_result(id, fixtures::command_output(text, Some(code), Ended::Exited), true)
}

/// The lines a read read, those of `text`, numbered from `first`, as what came back for the call `id`. Every line
/// between newlines is one, so a blank last line is read too.
pub fn read_back(id: &str, first: u32, text: &str) -> EventBody {
    let lines = text.split('\n').map(str::to_string).collect();
    typed_result(id, CallOutput::Lines(Lines { first, lines, total: None }), false)
}

/// What a write or an edit of the file at `path` changed in it, as what came back for the call `id`.
pub fn changed(id: &str, path: &str, change: FileChange) -> EventBody {
    let file = FileOutcome { path: path.into(), change: Some(change), moved_to: None, diagnostics: Vec::new() };
    typed_result(id, CallOutput::Changed { files: vec![file] }, false)
}

/// That the call `id` went on in the background, as a subagent's does that its call doesn't wait for.
pub fn launched(id: &str) -> EventBody {
    typed_result(id, CallOutput::Backgrounded { task: None }, false)
}

/// What a subagent reported as it ended, as what came back for the call `id` that waited for it.
pub fn reported(id: &str, report: &str) -> EventBody {
    typed_result(id, CallOutput::Report { report: report.into() }, false)
}

/// What an MCP tool sent back, `text`, as what came back for the call `id`, which failed if `is_error`.
pub fn mcp_said(id: &str, text: &str, is_error: bool) -> EventBody {
    let content = if text.is_empty() { Vec::new() } else { vec![McpContent::Text { text: text.into() }] };
    typed_result(id, CallOutput::Mcp { content, structured: None }, is_error)
}

/// That a write or an edit changed the file at `path`, as what came back for the call `id`, when its agent said nothing
/// of how, as Claude's CLI says nothing of a change too long to carry.
pub fn updated(id: &str, path: &str) -> EventBody {
    let file = FileOutcome { path: path.into(), change: None, moved_to: None, diagnostics: Vec::new() };
    typed_result(id, CallOutput::Changed { files: vec![file] }, false)
}

/// What came back for the call `id`, in words, as for a call with nothing of its own to send back.
pub fn said(id: &str, text: &str) -> EventBody {
    typed_result(id, words(text), false)
}

/// What went wrong, in words, as what came back for the call `id`, which failed.
pub fn failed(id: &str, text: &str) -> EventBody {
    typed_result(id, words(text), true)
}

/// What came back for the call `id` the person denied, which tells the agent `text`, with what they `said`, if anything.
pub fn denied(id: &str, text: &str, said: Option<&str>) -> EventBody {
    sent_back(fixtures::typed_denied(id, text, said))
}

/// A result that's only images, as a read of one is.
pub fn images(id: &str, images: Vec<Attachment>) -> EventBody {
    sent_back(fixtures::with_images(fixtures::typed_result(id, words(""), false), images))
}

fn words(text: &str) -> CallOutput {
    CallOutput::Words { text: text.to_string() }
}

fn sent_back(block: ContentBlock) -> EventBody {
    EventBody::AgentUser { subagent: None, blocks: vec![block] }
}

/// The end of a turn that went well. Opus answered, and read most of what it was sent from the cache, as a turn that
/// reads around a repository does.
pub fn finished_turn(seconds: u64, cost_usd: f64) -> EventBody {
    let tokens =
        TokenUsage { input_tokens: 6, output_tokens: 1_840, cache_read_tokens: 412_377, cache_write_tokens: 9_615 };
    EventBody::TurnEnded {
        outcome: TurnOutcome::Success,
        result: None,
        errors: Vec::new(),
        duration_ms: Some(seconds * 1_000),
        cost_usd: Some(cost_usd),
        num_turns: None,
        started_at_ms: None,
        models: vec![ModelName::named("claude-opus-5-5")],
        tokens: Some(tokens),
        only_compacted: false,
        request_failed: false,
    }
}

/// A turn that finished saying `reply`, as the CLI ends one, so its copy button has something to copy.
pub fn replied(reply: &str, seconds: u64, cost_usd: f64) -> EventBody {
    let mut ended = finished_turn(seconds, cost_usd);
    if let EventBody::TurnEnded { result, .. } = &mut ended {
        *result = Some(reply.to_string());
    }
    ended
}

/// `event` as the subagent started by the call `parent` sends it.
pub fn within(parent: &str, mut event: EventBody) -> EventBody {
    if let EventBody::Assistant { subagent, .. } | EventBody::AgentUser { subagent, .. } = &mut event {
        *subagent = Some(SubagentId::from(parent));
    }
    event
}

/// `event` as a reply from `model`, such as a subagent's from a model other than the agent's.
pub fn on(model: &str, mut event: EventBody) -> EventBody {
    if let EventBody::Assistant { model: from, .. } = &mut event {
        *from = Some(model.to_string());
    }
    event
}

/// The agent's word that a subagent started in the background, as the hub records it from the CLI's.
pub fn subagent_started(call: &str, kind: &str, description: &str, prompt: &str) -> EventBody {
    EventBody::SubagentStarted {
        call: call.into(),
        subagent: None,
        kind: Some(kind.into()),
        description: Some(description.into()),
        prompt: Some(prompt.into()),
        in_background: true,
    }
}

/// A task in the background as the hub lists it once the agent has said which call started it: what it's for, and
/// that call.
pub fn task(kind: TaskKind, call: &str, description: &str) -> AgentTask {
    AgentTask {
        id: format!("task-{call}").into(),
        description: Some(description.to_string()),
        kind,
        call: Some(ToolUseId::from(call)),
        subagent: None,
    }
}

/// The subagents working in the background, each by the call that started it and what it's for, as the hub records
/// the list once the agent has said which call started each.
pub fn subagents_listed(subagents: &[(&str, &str)]) -> EventBody {
    let tasks = subagents.iter().map(|(call, description)| task(TaskKind::Subagent, call, description));
    EventBody::AgentTasks { tasks: tasks.collect(), stopped: Vec::new() }
}

/// The agent's word on how many tokens the conversation of a subagent at work holds.
pub fn subagent_context(call: &str, tokens: u64) -> EventBody {
    EventBody::SubagentContext { subagent: call.into(), tokens }
}

/// The agent's word that a subagent finished, with its report and how many tokens its conversation held.
pub fn subagent_ended(call: &str, report: &str, tokens: u64) -> EventBody {
    let (outcome, report) = (TaskOutcome::Finished, Some(report.into()));
    EventBody::SubagentEnded { subagent: call.into(), outcome, report, context_tokens: Some(tokens) }
}
