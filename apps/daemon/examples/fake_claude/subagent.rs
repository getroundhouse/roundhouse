//! A subagent, started in the background as the real CLI starts one: the call's result only says it started, and
//! the subagent's own call and report come later, naming the call as their parent. Its report streams in a few
//! pieces, so a test can see what it's writing, and cut it off.

use std::collections::VecDeque;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use serde_json::json;

use crate::turn::{Block, Turn};
use crate::{Input, Prompt, pause};

/// How long a subagent takes before its work arrives, so a test can open its call first.
const WORKS_FOR: Duration = Duration::from_millis(1000);

/// Between the pieces of its report, so a test can catch it part way.
const WRITES_EVERY: Duration = Duration::from_millis(200);

/// Returns whether the turn was interrupted while the subagent worked.
pub(crate) fn start(
    turn: &mut Turn,
    description: &str,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<Prompt>,
    eof: &mut bool,
) -> bool {
    let input = json!({"description":description,"subagent_type":"Explore","prompt":"Look around."});
    let agent = turn.named_tool_use("Agent", input);
    turn.tool_result(&agent, "Async agent launched successfully.", false);
    if pause(rx, WORKS_FOR, queued, eof) {
        return true;
    }
    let mut within = Turn { parent: Some(&agent), ..*turn };
    let call = within.named_tool_use("Bash", json!({"command":"ls","description":"List what's here"}));
    within.tool_result(&call, "README.md", false);
    let report = ["There's ", "a README ", "here."].map(String::from).to_vec();
    within.say(Block::Text, report, WRITES_EVERY, rx, queued, eof)
}
