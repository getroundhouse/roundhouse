//! Chats whose agent asks to be allowed to use a tool, and their screens: one that's planning, which asks before it
//! runs a command, one with full access, whose subagents want to run a removal it asks about in any mode, and one that
//! asks to edit a file.

#![cfg(test)]

use protocol::capability::approvals::Asks;
use protocol::capability::calls::Call;
use protocol::capability::calls::call::{Command, Edit, Replace, Replaces, Runs};
use protocol::event::EventBody;
use protocol::fixtures::{ApprovalRequest, approval_request, decided, deny, typed_command, typed_request};
use protocol::model::{AgentKind, SessionStatus};

use super::frame::{CHAT, QUESTION};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::typed_chats;
use super::worlds::{chat_world, in_plan_mode};
use client_core::fake_hub::World;
use client_core::fake_hub::events::{
    command, denied as denied_with, first_turn, launched, replied, subagent, subagent_started, subagents_listed, text,
    typed_call, user_prompt, within,
};
use spec_screens::chats::{self, looked_around};

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
        name: "approval-denied",
        spec: "chats.md",
        world: Some(|| in_plan_mode(chat_world(SessionStatus::Idle, denied()))),
        frame: CHAT,
        steps: &[Step::Select(FOCUS)],
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
    Screen {
        name: "approval-patch",
        spec: "chats.md",
        world: Some(codex_patching),
        frame: QUESTION,
        steps: &[Step::Select(FOCUS)],
    },
];

/// A Codex chat waiting to be allowed [`patched`], on a hub that describes its agents as the app's own does, so the
/// request is asked as Codex takes an answer, which is without words.
fn codex_patching() -> World {
    let mut world = chat_world(SessionStatus::WaitingApproval, patched());
    world.agents = Some(agents::descriptions());
    for chat in &mut world.sessions {
        chat.agent = AgentKind::Codex;
    }
    world
}

/// Waiting to be allowed a patch of two files, as Codex's adapter asks: adding `may_pass` to the signal, and a depot
/// that uses it, with nothing said of what it's for.
fn patched() -> Vec<EventBody> {
    let call = Call::Patch(typed_chats::patch());
    first_turn(
        user_prompt("Add a `may_pass` to the signal, and a depot that uses it."),
        [
            text("msg-1", "I'll add it, and the depot."),
            typed_call("msg-2", "patch-1", call.clone()),
            typed_request("approval-1", "patch-1", call),
        ],
    )
}

/// Waiting to be allowed an edit to the project's settings for Claude, which says nothing of what it's for or why it's
/// asked.
fn edit() -> Vec<EventBody> {
    let mut events = looked_around();
    events.push(text("msg-3", "I'll let Clippy run without asking, as the tests do."));
    let replace = Replace {
        old: "    \"allow\": [\n      \"Bash(cargo test:*)\"\n    ]".into(),
        new: "    \"allow\": [\n      \"Bash(cargo test:*)\",\n      \"Bash(cargo clippy:*)\"\n    ]".into(),
        replaces: Replaces::One,
    };
    let edit = Call::Edit(Edit { path: "/workspaces/aspen/.claude/settings.json".into(), edits: vec![replace] });
    events.push(typed_call("msg-4", "toolu-3", edit.clone()));
    let request = ApprovalRequest {
        tool_use_id: Some("toolu-3".into()),
        ..approval_request("approval-1", Asks::Call { call: edit })
    };
    events.push(request.into());
    events
}

/// The command a chat that's planning asked to run, denied with what to do instead, and what the agent did then.
fn denied() -> Vec<EventBody> {
    let reply = "I'll leave the tests, then, and go by what the code says.";
    let instead = "Not yet. Read how the sidebar gets its records first";
    let mut events = chats::approval();
    events.push(decided("approval-1", deny(Some(instead))));
    events.push(denied_with("toolu-3", instead, Some(instead)));
    events.push(text("msg-5", reply));
    events.push(replied(reply, 41, 0.12));
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
        events.push(subagent("msg-4", call, "general-purpose", lane, &prompt, Runs::Background));
        events.push(subagent_started(call, "general-purpose", lane, &prompt));
    }
    events.push(subagents_listed(&LANES));
    for (call, _) in LANES {
        events.push(launched(call));
    }
    for (at, (call, _)) in LANES.iter().take(asking).enumerate() {
        let (run, approval) = (format!("toolu-run-{at}"), format!("approval-{at}"));
        let described = "Run the harness on a made-up guide and show what it draws";
        events.push(within(call, command(&format!("msg-run-{at}"), &run, COMMAND, Some(described))));
        let typed = Call::Command(Command { description: Some(described.to_string()), ..typed_command(COMMAND) });
        let request = ApprovalRequest {
            tool_use_id: Some(run.as_str().into()),
            asked_by: Some((*call).into()),
            ..approval_request(&approval, Asks::Call { call: typed })
        };
        let request = request.because(WHY);
        events.push(request.into());
    }
    events
}
