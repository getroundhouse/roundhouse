//! A subagent, started in the background as the real CLI starts one: the call's result only says it started, and
//! the subagent's own call and report come later, naming the call as their parent.

use std::collections::VecDeque;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use serde_json::json;

use crate::turn::{Turn, message_id};
use crate::{Input, pause};

/// How long a subagent takes before its work arrives, so a test can open its call first.
const WORKS_FOR: Duration = Duration::from_millis(1000);

/// Returns whether the turn was interrupted while the subagent worked.
pub(crate) fn start(
    turn: &mut Turn,
    description: &str,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<String>,
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
    within.finish_message(&message_id(), json!({"type":"text","text":"There's a README here."}));
    false
}
