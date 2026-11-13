//! The screens of what someone sees when the app or its hub updates, or when there's no hub, apart from the rest in
//! `screens.rs` only to keep each list a readable length.

#![cfg(test)]

use look::Region;
use protocol::event::EventBody;
use protocol::model::SessionStatus;

use super::events::{replied, taken, text, user_prompt};
use super::fake_hub::BUILD;
use super::frame::{CHAT, Frame, Part, SIDEBAR};
use super::screens::FOCUS;
use super::steps::{Screen, Step, Target};
use super::world::World;
use super::worlds::{chat_world, conversation, diff, finishing_update, own_hub};

pub(crate) const UPDATE_SCREENS: &[Screen] = &[
    Screen { name: "no-hub", spec: "updates.md", world: None, frame: Frame::Layout, steps: &[] },
    Screen {
        name: "update-ready",
        spec: "updates.md",
        world: Some(conversation),
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS), Step::UpdateReady("1.0.1")],
    },
    Screen {
        name: "restart-to-update-question",
        spec: "updates.md",
        world: Some(diff),
        frame: SIDEBAR,
        // The hub keeps what's typed and attached in a composer, and the gallery's hub is too old to keep a comment
        // being written, so it's the comment that restarting would lose.
        steps: &[
            Step::Select(FOCUS),
            Step::OpenFile("crates/protocol/src/model.rs"),
            Step::StartComment(99),
            Step::Type("Could this be an enum?"),
            Step::UpdateReady("1.0.1"),
            Step::Click("restart-to-update"),
        ],
    },
    Screen {
        name: "finishing-update",
        spec: "updates.md",
        world: Some(finishing_update),
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS)],
    },
    Screen {
        name: "update-details",
        spec: "updates.md",
        world: Some(finishing_update),
        frame: Frame::Exactly(Part::Region(Region::LeftSidebar), &[Part::Region(Region::UpdateDetails)]),
        steps: &[
            Step::Select(FOCUS),
            Step::Hover { on: Target::Id("update-details"), shows: Region::UpdateDetails.id() },
        ],
    },
    Screen {
        name: "finish-update-question",
        spec: "updates.md",
        world: Some(finishing_update),
        frame: Frame::Dialog,
        steps: &[Step::Select(FOCUS), Step::AskToFinish],
    },
    Screen { name: "own-hub", spec: "updates.md", world: Some(own_hub), frame: SIDEBAR, steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "update-problem",
        spec: "updates.md",
        world: Some(conversation),
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS), Step::UpdateProblem],
    },
    Screen { name: "updated", spec: "updates.md", world: Some(updated), frame: CHAT, steps: &[Step::Select(FOCUS)] },
];

/// A short chat, all of it in sight: a prompt whose agent started on the build before, which says nothing of builds,
/// then one sent once the app had updated, whose agent started on the new build, which does.
fn updated() -> World {
    let started = |pid, resumed, build: &str, previous_build: Option<&str>| EventBody::AgentStarted {
        pid,
        resumed,
        build: Some(build.to_string()),
        previous_build: previous_build.map(str::to_string),
    };
    let answer = "Yes. Each workspace's row has a badge for its pull request, from the workspace's record.";
    let reply = "Yes. The badge reads the state from the workspace's record, so it's right as soon as the feed says.";
    let events = vec![
        user_prompt("Does the sidebar show each workspace's pull request?"),
        started(4100, false, "0.9.0", None),
        taken(1),
        text("msg-1", answer),
        replied(answer, 14, 0.09),
        user_prompt("Does the badge update when the PR merges?"),
        started(4242, true, BUILD, Some("0.9.0")),
        taken(6),
        text("msg-2", reply),
        replied(reply, 9, 0.12),
    ];
    chat_world(SessionStatus::Idle, events)
}
