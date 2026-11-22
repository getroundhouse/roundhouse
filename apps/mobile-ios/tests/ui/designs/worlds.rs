//! What the hub holds for the designs' chats, which are the ones the desktop's screens show.

#![cfg(test)]

use client_core::fake_hub::World;
use client_core::fake_hub::events::{replied, taken, text, typed_call, user_prompt};
use protocol::capability::calls::Call;
use protocol::event::EventBody;
use protocol::fixtures::{ApprovalRequest, approval_request, plan};
use protocol::model::{Seq, SessionStatus, ToolUseId};
use spec_screens::{chats, question_chats};

/// The workspace every chat here is in.
pub(super) const WORKSPACE: &str = "aspen";

fn holding(status: SessionStatus, events: Vec<EventBody>) -> (World, protocol::model::SessionId) {
    let mut world = World::default();
    let repo = world.repo("roundhouse");
    let workspace = world.workspace(&repo, WORKSPACE);
    let chat = world.chat(&workspace, status, events);
    (world, chat)
}

/// A turn that's over.
pub(super) fn finished() -> World {
    let (mut world, chat) = holding(SessionStatus::Idle, chats::finished());
    world.last_turn_took(&chat, 108_000);
    world
}

/// Partway through a reply.
pub(super) fn working() -> World {
    let (mut world, chat) = holding(SessionStatus::Running, chats::streaming());
    world.working_for(&chat, 12_000);
    world.tails.insert(chat, chats::streaming_tail());
    world
}

/// Waiting on an answer to a question.
pub(super) fn asking() -> World {
    holding(SessionStatus::WaitingApproval, question_chats::question()).0
}

/// Waiting for leave to run a command.
pub(super) fn asking_to_run() -> World {
    holding(SessionStatus::WaitingApproval, chats::approval()).0
}

const PLAN: &str = "**Show each workspace's PR state in the sidebar**

1. Add `pull_request: Option<PullRequest>` to `Workspace`, which the hub fills in from GitHub for each branch.
2. Send it with the workspace's notice, so every client has it from the feed.
3. Draw it as a badge after the name in the sidebar's row, kept at its own width.
4. Draw each state of the badge in the gallery.";

/// In plan mode, with the agent asking for its plan to be approved, as the desktop's screen of it is.
pub(super) fn planned() -> World {
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
    holding(SessionStatus::WaitingApproval, events).0
}

const FIND: &str = "In /workspaces/aspen, find where the sidebar gets each workspace's record from, and what tells it \
                    a record changed.";
const FOUND: &str = "The sidebar reads `Workspace` records from the feed:\n\n\
                     - `apps/desktop/src/sidebar.rs` draws a row from each record.\n\
                     - `crates/hub/src/hub/workspaces.rs:224` sends `HubNotice::Workspace` whenever one changes.\n\n\
                     So a new field on `Workspace` reaches the sidebar without anything else.";

/// What a subagent did, as a chat of its own: what the agent asked it, and what it reported.
pub(super) fn subagent() -> World {
    let events = vec![user_prompt(FIND), taken(Seq::new(1)), text("msg-1", FOUND), replied(FOUND, 18, 0.06)];
    holding(SessionStatus::Idle, events).0
}
