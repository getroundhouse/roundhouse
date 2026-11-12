//! Chats whose agent compacted its conversation, and the screens about them, which the gallery takes after the
//! subagents': once when asked, once by itself partway through a turn, once still at it, and once when it couldn't, and
//! the outline's dots for them.

#![cfg(test)]

use protocol::event::{CompactedBy, CompactionOutcome, EventBody, TurnOutcome};
use protocol::model::{Seq, SessionStatus};
use protocol::tail::{Compacting, Tail};
use serde_json::json;

use super::chats::{finished, looked_around};
use super::events::{replied, taken, text, thought, tool_result, tool_use, user_prompt};
use super::frame::{CHAT, OUTLINE};
use super::screens::FOCUS;
use super::steps::{Screen, Step, Target};
use super::world::World;
use super::worlds::chat_world;

/// The screens about compacting.
pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "compacted",
        spec: "chats.md",
        frame: CHAT,
        world: Some(|| chat_world(SessionStatus::Idle, asked())),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "compacted-keeping",
        spec: "chats.md",
        frame: CHAT,
        world: Some(|| chat_world(SessionStatus::Idle, asked_keeping())),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "compacted-summary",
        spec: "chats.md",
        frame: CHAT,
        world: Some(|| chat_world(SessionStatus::Idle, asked_last())),
        steps: &[Step::Select(FOCUS), Step::Open(ASKED_LABEL)],
    },
    Screen {
        name: "compacted-by-itself",
        spec: "chats.md",
        frame: CHAT,
        world: Some(|| chat_world(SessionStatus::Idle, by_itself())),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "compacting",
        spec: "chats.md",
        frame: CHAT,
        world: Some(compacting_world),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "compaction-failed",
        spec: "chats.md",
        frame: CHAT,
        world: Some(|| chat_world(SessionStatus::Idle, failed())),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "outline-compactions",
        spec: "chats.md",
        frame: OUTLINE,
        world: Some(|| chat_world(SessionStatus::Idle, long_with_compactions())),
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "outline-compactions-list",
        spec: "chats.md",
        frame: OUTLINE,
        world: Some(|| chat_world(SessionStatus::Idle, long_with_compactions())),
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id(look::Region::Outline.id()), shows: "outline-list" },
        ],
    },
    Screen {
        name: "outline-compacted",
        spec: "chats.md",
        frame: OUTLINE,
        world: Some(|| chat_world(SessionStatus::Idle, by_itself())),
        steps: &[Step::Select(FOCUS)],
    },
];

/// The long chat's twenty turns, with the agent compacting by itself partway through the tenth, and someone asking it
/// to after the sixteenth.
fn long_with_compactions() -> Vec<EventBody> {
    let mut events = Vec::new();
    for (turn, (asked, said)) in (1..).zip(super::long_chats::TURNS) {
        let prompt = next(&events);
        events.extend([user_prompt(asked), taken(prompt)]);
        if turn == 10 {
            events.push(compacted(CompactedBy::Agent, 467_499, 15_694, 89_120));
        }
        events.extend([text(&format!("msg-{turn}"), said), replied(said, 12, 0.04)]);
        if turn == 16 {
            compact_asked(&mut events, None);
        }
    }
    events
}

/// The label of the compaction someone asked for in [`asked`] and [`asked_last`], which opens its summary.
const ASKED_LABEL: &str = "Compacted, 226.9k to 9.0k tokens, 51s";

/// What the agent carries on from, in the shape Claude writes one: numbered sections, each a few lines, so it's taller
/// than the box it opens in.
const SUMMARY: &str = "1. Primary Request and Intent:\n\
     \x20  Show each workspace's pull request state in the sidebar, as a badge on its row.\n\n\
     2. Key Technical Concepts:\n\
     \x20  - The hub owns what every client shows, so it asks GitHub, not the sidebar.\n\
     \x20  - Clients get each `Workspace` whole from the feed.\n\n\
     3. Files and Code Sections:\n\
     \x20  - `crates/hub/src/hub/workspaces.rs`, where workspace notices are sent.\n\
     \x20  - `crates/protocol/src/model.rs`, which defines `Workspace`.\n\
     \x20  - `apps/desktop/src/sidebar.rs`, which draws a row from `Workspace` alone.\n\n\
     4. Errors and fixes:\n\
     \x20  - None yet.\n\n\
     5. Problem Solving:\n\
     \x20  - Asking GitHub from the hub means it's asked once, however many clients are open.\n\n\
     6. All user messages:\n\
     \x20  - “Show the PR state in the sidebar.”\n\
     \x20  - “Go with the first.”\n\n\
     7. Pending Tasks:\n\
     \x20  - Add `pull_request: Option<PullRequest>` to `Workspace`.\n\
     \x20  - Fill it in on the hub, and draw the badge.\n\n\
     8. Current Work:\n\
     \x20  Choosing between a field on `Workspace` and a feed of its own.\n\n\
     9. Optional Next Step:\n\
     \x20  Add the field, then fill it in on the hub.";

/// The conversation, then a compaction someone asked for, then the next turn, which carries on from the summary.
fn asked() -> Vec<EventBody> {
    asked_as(None)
}

/// The same, saying what to keep.
fn asked_keeping() -> Vec<EventBody> {
    asked_as(Some("Keep the decision to put the PR state on Workspace, and the files we looked at"))
}

/// The conversation, then a compaction someone asked for, with nothing after it, so its summary opens in view.
fn asked_last() -> Vec<EventBody> {
    let mut events = finished();
    compact_asked(&mut events, None);
    events
}

/// The conversation, then a compaction asked for keeping `keeping`, then the next turn.
fn asked_as(keeping: Option<&str>) -> Vec<EventBody> {
    let mut events = finished();
    compact_asked(&mut events, keeping);
    let prompt = next(&events);
    let reply = "The field is on `Workspace`, and the sidebar draws a badge from it. Nothing else had to change, \
                 since the feed already sends the whole record.";
    events.extend([
        user_prompt("Go with the first. Add the field, and have the sidebar draw a badge from it."),
        taken(prompt),
        text("msg-5", "I'll add the field first."),
        tool_use(
            "msg-5",
            "toolu-5",
            "Edit",
            json!({ "file_path": "/workspaces/aspen/crates/protocol/src/model.rs",
                "old_string": "    pub branch: String,\n", "new_string": "    pub branch: String,\n    pub pull_request: Option<PullRequest>,\n" }),
        ),
        tool_result("toolu-5", "The file has been updated."),
        text("msg-6", reply),
        replied(reply, 41, 0.12),
    ]);
    events
}

/// A compaction asked for keeping `keeping`, after `events`, as the hub and the adapter record it.
fn compact_asked(events: &mut Vec<EventBody>, keeping: Option<&str>) {
    let asked = next(events);
    events.extend([
        EventBody::CompactionAsked { keeping: keeping.map(str::to_string) },
        taken(asked),
        asked_for(compacted(CompactedBy::Asked, 226_869, 9_009, 51_558), asked, keeping),
        only_compacted(51_797),
    ]);
}

/// Partway through a turn, with the context full, so the agent compacts by itself before going on.
fn by_itself() -> Vec<EventBody> {
    let mut events = so_far();
    events.push(compacted(CompactedBy::Agent, 467_499, 15_694, 89_120));
    let reply = "The badge is in. Every client gets it from the feed, and the sidebar's tests pass.";
    events.extend([
        thought("msg-6", "The summary says the field is added, so the sidebar is what's left."),
        tool_use(
            "msg-6",
            "toolu-6",
            "Bash",
            json!({ "command": "cargo test -p desktop --test ui sidebar", "description": "Run the sidebar's tests" }),
        ),
        tool_result("toolu-6", "test result: ok. 31 passed; 0 failed"),
        text("msg-7", reply),
        replied(reply, 214, 0.86),
    ]);
    events
}

/// Partway through a turn, compacting, three minutes in.
fn compacting_world() -> World {
    let mut events = so_far();
    events.push(EventBody::AgentCompacting { written_chars: Some(16_464) });
    let mut world = chat_world(SessionStatus::Running, events);
    let chat = world.sessions.last().unwrap().id.clone();
    let compacting = Compacting { written_chars: Some(16_464), ..Compacting::default() };
    world.tails.insert(chat.clone(), Tail { compacting: Some(compacting), ..Tail::default() });
    world.working_for(&chat, 187_300);
    world
}

/// The conversation, then a compaction asked for that the agent couldn't do, as the CLI says when there's too little.
fn failed() -> Vec<EventBody> {
    let mut events = finished();
    let asked = next(&events);
    let couldnt = EventBody::AgentCompacted {
        asked: Some(asked),
        keeping: None,
        outcome: CompactionOutcome::Failed { error: Some("Not enough messages to compact.".to_string()) },
    };
    events.extend([EventBody::CompactionAsked { keeping: None }, taken(asked), couldnt, only_compacted(212)]);
    events
}

/// The end of a turn that was only a compaction someone asked for, which has nothing to say.
fn only_compacted(duration_ms: u64) -> EventBody {
    EventBody::TurnEnded {
        outcome: TurnOutcome::Success,
        result: Some(String::new()),
        errors: Vec::new(),
        duration_ms: Some(duration_ms),
        cost_usd: Some(0.0),
        num_turns: Some(0),
        started_at_ms: None,
        models: Vec::new(),
        tokens: None,
        only_compacted: true,
    }
}

/// A turn that has looked around and added the field, and is about to draw the badge.
fn so_far() -> Vec<EventBody> {
    let mut events = looked_around();
    events.extend([
        text("msg-3", "I'll add the field to `Workspace`, then draw the badge."),
        tool_use(
            "msg-3",
            "toolu-3",
            "Edit",
            json!({ "file_path": "/workspaces/aspen/crates/protocol/src/model.rs",
                "old_string": "    pub branch: String,\n", "new_string": "    pub branch: String,\n    pub pull_request: Option<PullRequest>,\n" }),
        ),
        tool_result("toolu-3", "The file has been updated."),
        tool_use(
            "msg-4",
            "toolu-4",
            "Bash",
            json!({ "command": "cargo check --workspace", "description": "Check that everything still builds" }),
        ),
        tool_result("toolu-4", "Finished `dev` profile [unoptimized + debuginfo] target(s) in 41.20s"),
    ]);
    events
}

fn compacted(by: CompactedBy, before: u64, after: u64, took_ms: u64) -> EventBody {
    let outcome = CompactionOutcome::Compacted {
        by,
        before_tokens: Some(before),
        after_tokens: Some(after),
        took_ms: Some(took_ms),
        summary: Some(SUMMARY.to_string()),
    };
    EventBody::AgentCompacted { asked: None, keeping: None, outcome }
}

/// `compacted` as the compaction asked for at `asked`, saying `keeping`.
fn asked_for(mut compacted: EventBody, asked: Seq, keeping: Option<&str>) -> EventBody {
    if let EventBody::AgentCompacted { asked: named, keeping: kept, .. } = &mut compacted {
        *named = Some(asked);
        *kept = keeping.map(str::to_string);
    }
    compacted
}

/// The seq the next event pushed onto `events` gets.
fn next(events: &[EventBody]) -> Seq {
    Seq::try_from(events.len()).unwrap() + 1
}
