//! The screens of what someone sees when the app or its hub updates, or when there's no hub, and of which version a
//! workspace is on, apart from the rest in `screens.rs` only to keep each list a readable length.

#![cfg(test)]

use desktop::updates::{Kind, Refusal};
use look::Region;
use protocol::event::EventBody;
use protocol::model::SessionStatus;

use super::events::{replied, taken, text, user_prompt};
use super::fake_hub::BUILD;
use super::frame::{CHAT, Frame, Part, SIDEBAR, WORKSPACE_CARD};
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
        steps: &[Step::Select(FOCUS), Step::UpdateProblem(Kind::Unreachable, "connection refused")],
    },
    Screen {
        name: "update-refused",
        spec: "updates.md",
        world: Some(conversation),
        frame: SIDEBAR,
        steps: &[
            Step::Select(FOCUS),
            Step::UpdateProblem(Kind::Refused(Refusal::Unsigned), "the update feed's signature doesn't match it"),
        ],
    },
    Screen {
        name: "update-translocated",
        spec: "updates.md",
        world: Some(conversation),
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS), Step::UpdateProblem(Kind::Translocated, "opened where it was downloaded")],
    },
    Screen {
        name: "checking-for-updates",
        spec: "updates.md",
        world: Some(conversation),
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS), Step::UpdateChecking],
    },
    Screen { name: "updated", spec: "updates.md", world: Some(updated), frame: CHAT, steps: &[Step::Select(FOCUS)] },
    Screen {
        name: "workspace-card",
        spec: "workspaces.md",
        world: Some(under_a_home_folder),
        frame: WORKSPACE_CARD,
        steps: &[Step::Select(FOCUS), Step::Hover { on: Target::Id(TITLE), shows: Region::WorkspaceCard.id() }],
    },
    Screen {
        name: "workspace-card-moving",
        spec: "updates.md",
        world: Some(on_the_older_build),
        frame: WORKSPACE_CARD,
        steps: &[Step::Select(FOCUS), Step::Hover { on: Target::Id(TITLE), shows: Region::WorkspaceCard.id() }],
    },
];

/// The workspace's name in its header, which the card opens from.
const TITLE: &str = "workspace-title";

/// The conversation, in a workspace under a home folder, as one is outside the gallery, with a path too long for the
/// card.
fn under_a_home_folder() -> World {
    let mut world = conversation();
    let shown = world.workspaces.iter_mut().find(|workspace| workspace.name == FOCUS).unwrap();
    shown.path = Some(format!("/Users/sam/Developer/roundhouse/workspaces/signals/{FOCUS}"));
    world
}

/// The same, in a workspace whose worker is still the build before the hub's, as after an update while something in
/// it was at work.
fn on_the_older_build() -> World {
    let mut world = under_a_home_folder();
    let shown = world.workspaces.iter_mut().find(|workspace| workspace.name == FOCUS).unwrap();
    shown.worker_build = Some("0.9.0".to_string());
    world
}

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
