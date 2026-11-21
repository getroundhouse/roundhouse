//! More of the screens about what a chat's calls show, a plan waiting to be approved, and the context while the agent
//! works, each of a state its rules in `chats.md` say, apart from the rest only to keep each list a readable length.

#![cfg(test)]

use look::Region;
use protocol::capability::calls::Call;
use protocol::event::EventBody;
use protocol::fixtures::{ApprovalRequest, approval_request, plan};
use protocol::model::{SessionStatus, ToolUseId};

use super::frame::{CHAT, CONTEXT};
use super::screens::FOCUS;
use super::steps::{Screen, Step, Target};
use super::worlds::{chat_world, context, in_plan_mode};
use super::{chats, tool_chats};
use client_core::fake_hub::World;
use client_core::fake_hub::events::{first_turn, printed, replied, text, typed_call, user_prompt};

pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "tool-output-alone",
        spec: "chats.md",
        world: Some(|| chat_world(SessionStatus::Idle, output_alone())),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open("Output")],
    },
    Screen {
        name: "tool-advisor-waiting",
        spec: "chats.md",
        world: Some(advisor_waiting),
        frame: CHAT,
        steps: &[Step::Select(FOCUS), Step::Open("Ask the advisor")],
    },
    Screen {
        name: "plan-approval",
        spec: "chats.md",
        world: Some(|| in_plan_mode(chat_world(SessionStatus::WaitingApproval, planned()))),
        frame: CHAT,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "context-working",
        spec: "chats.md",
        world: Some(context_working),
        frame: CONTEXT,
        steps: &[
            Step::Select(FOCUS),
            Step::WaitFor("usage"),
            Step::Hover { on: Target::Id("context"), shows: Region::ContextCard.id() },
        ],
    },
];

/// What the sidebar's tests printed, as a background run's result comes back to a chat that no longer has its call.
const TEST_OUTPUT: &str = "running 42 tests
test sidebar::rows_follow_the_hub ... ok
test sidebar::badge_keeps_its_width ... FAILED
test sidebar::hidden_projects_stay_hidden ... ok

failures:
    sidebar::badge_keeps_its_width

test result: FAILED. 41 passed; 1 failed; 0 ignored";

/// A turn whose first event is a result from a call too far back for the chat to have, so it's drawn on its own.
fn output_alone() -> Vec<EventBody> {
    let reply = "One failed: `badge_keeps_its_width`. The badge is drawn after the name, which doesn't shrink, so at \
                 this width it runs over the name.";
    first_turn(
        user_prompt("Did the sidebar's tests pass in the run you started?"),
        [printed("toolu-0", TEST_OUTPUT), text("msg-1", reply), replied(reply, 6, 0.02)],
    )
}

/// The agent at work, having asked the advisor, which hasn't replied yet.
fn advisor_waiting() -> World {
    let events = first_turn(
        user_prompt(tool_chats::ASK),
        [
            text("msg-1", "Before I change `Aspect`, I'll ask the advisor whether the plan holds up."),
            typed_call("msg-2", "srvtoolu-1", Call::Advisor),
        ],
    );
    let mut world = chat_world(SessionStatus::Running, events);
    let chat = world.sessions.last().unwrap().id.clone();
    world.working_for(&chat, 64_200);
    world
}

/// What the agent plans in [`planned`].
const PLAN: &str = "**Show each workspace's PR state in the sidebar**

1. Add `pull_request: Option<PullRequest>` to `Workspace`, which the hub fills in from GitHub for each branch.
2. Send it with the workspace's notice, so every client has it from the feed.
3. Draw it as a badge after the name in the sidebar's row, kept at its own width.
4. Draw each state of the badge in the gallery.";

/// A chat in plan mode, its agent having looked around and asking for its plan to be approved.
fn planned() -> Vec<EventBody> {
    let mut events = chats::looked_around();
    for event in &mut events {
        if let EventBody::AgentInit { mode, .. } = event {
            *mode = Some(protocol::fixtures::plan_mode());
        }
    }
    events.push(text("msg-3", "That's all it needs. Here's the plan."));
    events.push(typed_call("msg-4", "toolu-3", Call::Plan));
    events.push(
        ApprovalRequest { tool_use_id: Some(ToolUseId::from("toolu-3")), ..approval_request("approval-1", plan(PLAN)) }
            .into(),
    );
    events
}

/// The context's card, with the agent at work on a turn, when it can't be compacted.
fn context_working() -> World {
    let mut world = context();
    let chat = world.sessions.last_mut().unwrap();
    chat.status = SessionStatus::Running;
    let id = chat.id.clone();
    world.working_for(&id, 21_400);
    world
}
