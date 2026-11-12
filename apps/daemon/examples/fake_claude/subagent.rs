//! A subagent, started in the background as the real CLI starts one: the call's result only says it started, and
//! the subagent's own calls and report come later, naming the call as their parent. It makes one call, or as many as
//! it's asked for, for a subagent tab long enough to scroll. Its report streams in a few pieces, so a test can see
//! what it's writing, and cut it off. The CLI's word on the task it runs as comes around that, as the real CLI's does:
//! that it started, after the calls, and that it finished, after its report. One in the foreground is the same, but
//! for the call's result, which comes last, with the report.

use std::collections::VecDeque;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use serde_json::json;

use crate::turn::{Block, Turn};
use crate::{Input, Prompt, emit, pause};

/// How long a subagent takes before its work arrives, so a test can open its call first.
const WORKS_FOR: Duration = Duration::from_millis(1000);

/// Between the pieces of its report, so a test can catch it part way.
const WRITES_EVERY: Duration = Duration::from_millis(200);

/// Returns whether the turn was interrupted while the subagent worked.
pub(crate) fn start(
    turn: &mut Turn,
    description: &str,
    in_background: bool,
    calls: usize,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<Prompt>,
    eof: &mut bool,
) -> bool {
    let input = json!({"description":description,"subagent_type":"Explore","prompt":"Look around."});
    let agent = turn.named_tool_use("Agent", input);
    let (task, session_id) = (format!("task_{agent}"), &turn.config.session_id);
    emit(
        turn.out,
        json!({"type":"system","subtype":"task_started","task_id":task,"tool_use_id":agent,"description":description,
        "task_type":"local_agent","subagent_type":"Explore","prompt":"Look around.","is_backgrounded":in_background,
        "session_id":session_id}),
    );
    if in_background {
        turn.tool_result(&agent, "Async agent launched successfully.", false);
    }
    if pause(rx, WORKS_FOR, queued, eof) {
        return true;
    }
    let mut within = Turn { parent: Some(&agent), ..*turn };
    for _ in 0..calls {
        let call = within.named_tool_use("Bash", json!({"command":"ls","description":"List what's here"}));
        within.tool_result(&call, "README.md", false);
    }
    let report = ["There's ", "a README ", "here."].map(String::from).to_vec();
    if within.say(Block::Text, report, WRITES_EVERY, rx, queued, eof) {
        return true;
    }
    emit(
        turn.out,
        json!({"type":"system","subtype":"task_notification","task_id":task,"tool_use_id":agent,"status":"completed",
        "summary":"There's a README here.","session_id":session_id}),
    );
    if !in_background {
        turn.tool_result(&agent, "There's a README here.", false);
    }
    false
}
