//! Typed calls and what came back for them, as an adapter that types its calls records them.

use serde_json::Value;

use crate::attachment::{Attachment, MediaType};
use crate::capability::approvals::{Asks, Request};
use crate::capability::calls::call::{Command, Runs, StartSubagent};
use crate::capability::calls::output::{CommandOutput, Ended};
use crate::capability::calls::{Call, CallOutput};
use crate::event::{ContentBlock, Denied, EventBody};

/// The call `id`, as the block an adapter that types its calls makes it in.
pub fn typed_call(id: &str, call: Call) -> ContentBlock {
    ContentBlock::Call { id: id.into(), call }
}

/// Running `command`, which the agent didn't describe, in the conversation's own directory, waiting for it to end, for
/// a test to set what it's about and wrap in [`Call::Command`].
pub fn typed_command(command: &str) -> Command {
    Command {
        command: command.into(),
        cwd: None,
        description: None,
        timeout_ms: None,
        runs: Runs::Waits,
        parts: Vec::new(),
        unsandboxed: None,
    }
}

/// Handing `prompt` to a subagent of the agent's default persona, which the agent didn't describe, waiting for it to
/// end, for a test to set what it's about and wrap in [`Call::Subagent`].
pub fn typed_subagent(prompt: &str) -> StartSubagent {
    StartSubagent {
        persona: None,
        description: None,
        prompt: prompt.into(),
        model: None,
        effort: None,
        runs: Runs::Waits,
        resumes: None,
    }
}

/// What came back for the call `id`, typed, as the block it comes in: `output`, and whether it failed. Nothing came back
/// beside it, and it isn't said to be denied.
pub fn typed_result(id: &str, output: CallOutput, is_error: bool) -> ContentBlock {
    ContentBlock::CallResult { tool_use_id: id.into(), output, is_error, denied: None, images: Vec::new() }
}

/// What came back for the typed call `id` the person denied: what the agent was told, `text`, as a failure, and what
/// the person said, if anything.
pub fn typed_denied(id: &str, text: &str, said: Option<&str>) -> ContentBlock {
    let output = CallOutput::Words { text: text.into() };
    let denied = Some(Denied { said: said.map(str::to_string) });
    ContentBlock::CallResult { tool_use_id: id.into(), output, is_error: true, denied, images: Vec::new() }
}

/// What a command printed, `printed`, and how it ended, with its exit code if it exited with one. It says nothing of
/// how long it took, and kept all of what it printed.
pub fn command_output(printed: &str, exit_code: Option<i32>, ended: Ended) -> CallOutput {
    CallOutput::Command(CommandOutput { output: printed.into(), exit_code, ended, took_ms: None, truncated: None })
}

/// The agent asking, under the approval `id`, to make the typed call `call`, whose ID is `call_id`, as an adapter that
/// types its calls asks: naming the call, but with none of what else a request from before typed calls says beside
/// it, and no reason.
pub fn typed_request(id: &str, call_id: &str, call: Call) -> EventBody {
    let request = Request { asks: Asks::Call { call }, suggested: Vec::new(), always: false, why: None };
    EventBody::ApprovalRequested {
        approval_id: id.into(),
        request: Some(Box::new(request)),
        tool_name: String::new(),
        tool_use_id: Some(call_id.into()),
        input: Value::Null,
        description: None,
        suggestions: Box::new(Value::Null),
        asking: None,
        reason: None,
        asked_by: None,
    }
}

/// A picture the hub keeps as the attachment `id`, as one a tool sent back is.
pub fn image(id: &str) -> Attachment {
    Attachment { id: id.into(), name: format!("{id}.png"), media_type: MediaType::Png, size_bytes: 1 }
}

/// The result `block`, with `images` come back with it, beside what it holds.
///
/// # Panics
///
/// If `block` isn't a result, since only a result has images that came back.
pub fn with_images(mut block: ContentBlock, images: Vec<Attachment>) -> ContentBlock {
    match &mut block {
        ContentBlock::CallResult { images: kept, .. } => *kept = images,
        ContentBlock::Text { .. }
        | ContentBlock::Thinking { .. }
        | ContentBlock::Call { .. }
        | ContentBlock::Image { .. }
        | ContentBlock::Unknown => panic!("only a result has images that came back: {block:?}"),
    }
    block
}
