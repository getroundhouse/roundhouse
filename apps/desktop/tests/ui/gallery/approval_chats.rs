//! Chats whose agent asks to be allowed to use a tool, and their screens: one that's planning, which asks before it
//! runs a command, one with full access, whose subagents want to run a removal it asks about in any mode, and one that
//! asks to edit a file.

#![cfg(test)]

use protocol::event::{Asking, CallKind, EventBody};
use protocol::fixtures::{ApprovalRequest, approval_request};
use protocol::model::SessionStatus;
use serde_json::json;

use super::chats::{self, looked_around};
use super::events::{subagent_started, subagents_listed, text, tool_result, tool_use, within};
use super::frame::QUESTION;
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::worlds::{chat_world, in_plan_mode};

/// The screens about a tool's use the agent asks about.
pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "approval",
        spec: "chats.md",
        world: Some(|| in_plan_mode(chat_world(SessionStatus::WaitingApproval, chats::approval()))),
        frame: QUESTION,
        steps: &[Step::Select(FOCUS), Step::Click("question-answer-0")],
    },
    Screen {
        name: "approval-full-access",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::WaitingApproval, removal(1))),
        frame: QUESTION,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "approval-denying",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::WaitingApproval, removal(3))),
        frame: QUESTION,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("question-other"),
            Step::Type("Make the directory with mktemp -d, and delete that by its full path"),
        ],
    },
    Screen {
        name: "approval-edit",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::WaitingApproval, edit())),
        frame: QUESTION,
        steps: &[Step::Select(FOCUS)],
    },
];

/// Waiting to be allowed an edit to the project's settings for Claude, which says nothing of what it's for or why it's
/// asked.
fn edit() -> Vec<EventBody> {
    let mut events = looked_around();
    events.push(text("msg-3", "I'll let Clippy run without asking, as the tests do."));
    let input = json!({
        "file_path": "/workspaces/aspen/.claude/settings.json",
        "old_string": "    \"allow\": [\n      \"Bash(cargo test:*)\"\n    ]",
        "new_string": "    \"allow\": [\n      \"Bash(cargo test:*)\",\n      \"Bash(cargo clippy:*)\"\n    ]",
    });
    events.push(tool_use("msg-4", "toolu-3", "Edit", input.clone()));
    let request = ApprovalRequest {
        tool_use_id: Some("toolu-3".into()),
        input,
        asking: Some(Asking::ToolUse { call: Some(CallKind::Edit) }),
        ..approval_request("approval-1", "Edit")
    };
    events.push(request.into());
    events
}

/// What the reviewing subagents are for, each by the call that started it.
const LANES: [(&str, &str); 3] = [
    ("toolu-3", "Review lane: correctness"),
    ("toolu-4", "Review lane: tests"),
    ("toolu-5", "Review lane: architecture"),
];

const COMMAND: &str = "T=$PWD/.context/tmp/review; H=$PWD/target/review-harness/debug/guide-harness
rm -rf \"$(cat $T/last-run)\"; mkdir -p $T/r1/docs/guide
cat > $T/r1/docs/guide/README.md <<'END'
# Guide

- [A](a.md)
- [B](b.md)
END
$H $T/r1 $T/r1/out && sed -n '/<article>/,/<\\/article>/p' $T/r1/out/a/index.html";

/// Why Claude's CLI asks about a removal like that one, in its words.
const WHY: &str = "Dangerous rm operation on statically-unresolvable target: command substitution output";

/// Waiting to be allowed a removal that each of the first `asking` of three reviewing subagents wants to run, the
/// first of them first.
fn removal(asking: usize) -> Vec<EventBody> {
    let mut events = looked_around();
    events.push(text("msg-3", "I'll have the change reviewed before going on."));
    for (call, lane) in LANES {
        let prompt = format!("Review the change for {}.", lane.trim_start_matches("Review lane: "));
        let input = json!({ "description": lane, "subagent_type": "general-purpose", "prompt": prompt });
        events.push(tool_use("msg-4", call, "Agent", input));
        events.push(subagent_started(call, "general-purpose", lane, &prompt));
    }
    events.push(subagents_listed(&LANES));
    for (call, _) in LANES {
        events.push(tool_result(call, "Async agent launched successfully."));
    }
    for (at, (call, _)) in LANES.iter().take(asking).enumerate() {
        let (run, approval) = (format!("toolu-run-{at}"), format!("approval-{at}"));
        let described = "Run the harness on a made-up guide and show what it draws";
        let input = json!({ "command": COMMAND, "description": described });
        events.push(within(call, tool_use(&format!("msg-run-{at}"), &run, "Bash", input.clone())));
        let request = ApprovalRequest {
            tool_use_id: Some(run.as_str().into()),
            input,
            description: Some(described.to_string()),
            asking: Some(Asking::ToolUse { call: Some(CallKind::Command) }),
            reason: Some(WHY.to_string()),
            asked_by: Some((*call).into()),
            ..approval_request(&approval, "Bash")
        };
        events.push(request.into());
    }
    events
}
