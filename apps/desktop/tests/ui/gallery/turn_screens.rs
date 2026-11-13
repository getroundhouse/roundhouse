//! More of the screens about a chat's turns, what's sent while the agent works, and a chat's title, each of a state
//! its rules in `chats.md` say, apart from the rest in `screens.rs` only to keep each list a readable length.

#![cfg(test)]

use protocol::event::{EventBody, TurnOutcome};
use protocol::model::{Seq, SessionStatus};
use serde_json::json;

use super::chats;
use super::events::{taken, text, tool_result, tool_use, user_prompt};
use super::frame::{CHAT, Frame, QUEUE, TABS};
use super::screens::FOCUS;
use super::steps::{Screen, Step, Target};
use super::world::World;
use super::worlds::{chat_world, conversation, queued};

/// The tab of the chat nothing has been asked in yet, the seventh made in [`untitled`].
const UNTITLED_TAB: &str = "chat-chat-7";

pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "turn-endings",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, endings())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "prompt-dropped",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, dropped())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "queued-changing",
        spec: "chats.md",
        world: Some(queued),
        frame: QUEUE,
        steps: &[
            Step::Select(FOCUS),
            // The first, which carries a file, which stays beside what's changed.
            Step::Hover { on: Target::Id("queued-queued-1"), shows: "edit-queued-queued-1" },
            Step::Click("edit-queued-queued-1"),
            Step::MoveAway,
        ],
    },
    Screen {
        name: "untitled-chat",
        spec: "chats.md",
        world: Some(untitled),
        frame: TABS,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "rename-untitled-chat",
        spec: "chats.md",
        world: Some(untitled),
        frame: Frame::Dialog,
        steps: &[Step::Select(FOCUS), Step::RightClick(UNTITLED_TAB), Step::ClickItem("Rename…")],
    },
];

/// Three turns, one ending each way a turn ends: done, stopped, and failed.
fn endings() -> Vec<EventBody> {
    let mut events = chats::answered(
        "Which colours does GitHub use for a PR's state?",
        "Green for open, purple for merged, red for closed, and grey for a draft.",
    );
    let second = next_seq(&events);
    events.extend([
        user_prompt("Run the sidebar's tests, to see where they stand."),
        taken(second),
        text("msg-2", "I'll run the desktop app's sidebar tests first."),
        tool_use("msg-3", "toolu-1", "Bash", json!({ "command": "cargo test -p desktop --test ui sidebar" })),
        tool_result("toolu-1", "test result: ok. 41 passed; 0 failed; 0 ignored"),
        ended(TurnOutcome::Cancelled, Vec::new(), 14_200),
    ]);
    let third = next_seq(&events);
    events.extend([
        user_prompt("Then run the whole UI suite."),
        taken(third),
        ended(
            TurnOutcome::Error,
            vec!["API Error: 529 the servers are overloaded, try again in a minute".into()],
            1_200,
        ),
    ]);
    events
}

/// Running the UI tests, with a prompt sent meanwhile, then stopped before the agent read it.
fn dropped() -> Vec<EventBody> {
    let mut events = chats::steered();
    let steer = Seq::new(u64::try_from(events.len()).unwrap());
    events.extend([EventBody::PromptDropped { prompt: steer }, ended(TurnOutcome::Cancelled, Vec::new(), 31_600)]);
    events
}

/// The conversation, and a chat in its workspace that nothing has been asked in yet, so it has no title.
fn untitled() -> World {
    let mut world = conversation();
    let focus = world.workspaces.iter().find(|workspace| workspace.name == FOCUS).unwrap().id.clone();
    world.chat(&focus, SessionStatus::Idle, Vec::new());
    world
}

/// The seq the next event pushed onto `events` will have, as [`World::chat`] numbers them.
fn next_seq(events: &[EventBody]) -> Seq {
    Seq::new(u64::try_from(events.len() + 1).unwrap())
}

/// The end of a turn that went no further than `outcome` says, with what went wrong, after `took_ms`.
fn ended(outcome: TurnOutcome, errors: Vec<String>, took_ms: u64) -> EventBody {
    EventBody::TurnEnded {
        outcome,
        result: None,
        errors,
        duration_ms: Some(took_ms),
        cost_usd: None,
        num_turns: None,
        started_at_ms: None,
        models: Vec::new(),
        tokens: None,
        only_compacted: false,
        request_failed: false,
    }
}
