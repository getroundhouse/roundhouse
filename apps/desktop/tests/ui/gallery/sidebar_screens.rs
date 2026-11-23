//! More of the sidebar's screens, each of a state its rules in `sidebar.md` say, apart from the rest in `screens.rs`
//! only to keep each list a readable length.

#![cfg(test)]

use gpui_kit::px;
use protocol::model::Seq;

use super::frame::{Frame, SIDEBAR, SIDEBAR_MENU};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::worlds::{conversation, with_lines};
use client_core::fake_hub::{PairingWorld, World};

/// What's typed in the screen's own workspace and left unsent.
const DRAFT: &str = "Keep the badge at its own width";
const SAMPLE_PAIR_LINK: &str = "roundhouse://pair?v=1&name=Mac%20Studio&fp=q3B0ZWxsZXJzLWZpbmdlcnByaW50LXNoYTI1Ni1oYXN&code=9f3k-7m2p&host=mac-studio.local:7867&host=100.64.0.7:7867";

pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "sidebar-on-home",
        spec: "sidebar.md",
        world: Some(|| with_lines(conversation())),
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS), Step::Type(DRAFT), Step::Click("home"), Step::MoveAway],
    },
    Screen {
        name: "sort-menu",
        spec: "sidebar.md",
        world: Some(conversation),
        frame: SIDEBAR_MENU,
        steps: &[Step::Select(FOCUS), Step::Point("workspaces-heading"), Step::Click("sort-workspaces")],
    },
    Screen {
        name: "sidebar-edge",
        spec: "sidebar.md",
        world: Some(conversation),
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS), Step::Point("left-sidebar-edge")],
    },
    Screen {
        name: "row-end",
        spec: "sidebar.md",
        world: Some(expiring),
        frame: SIDEBAR,
        // Typed in the screen's own workspace, the fourth, which is then left for another, so its row has a pencil.
        steps: &[Step::Select(FOCUS), Step::Type(DRAFT), Step::Select("alder"), Step::Point("workspace-ws-4")],
    },
    Screen {
        name: "project-collapsed",
        spec: "sidebar.md",
        world: Some(conversation),
        // The first project is collapsed, with a workspace that has a reply nobody has read, and the pointer is left
        // on the second project's row.
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS), Step::Click("project-repo-1"), Step::Point("project-repo-2")],
    },
    Screen {
        name: "project-dragged",
        spec: "sidebar.md",
        world: Some(conversation),
        // The first project is held partway down over the second, not yet past half of it, so its own place is
        // still empty and the second hasn't moved.
        frame: SIDEBAR,
        steps: &[Step::Select(FOCUS), Step::Drag("project-repo-1", px(54.))],
    },
    Screen {
        name: "hub-switcher-menu",
        spec: "sidebar.md",
        world: Some(with_saved_hub),
        frame: SIDEBAR_MENU,
        steps: &[Step::Select(FOCUS), Step::Click("hub-switcher"), Step::MoveAway],
    },
    Screen {
        name: "connect-hub",
        spec: "sidebar.md",
        world: Some(conversation),
        frame: Frame::Dialog,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("hub-switcher"),
            Step::ClickItem("Connect another hub…"),
            Step::WaitFor("connect-hub-dialog"),
            Step::Type(SAMPLE_PAIR_LINK),
            Step::WaitFor("pair-link-device-name"),
            Step::MoveAway,
        ],
    },
    Screen {
        name: "pair-device",
        spec: "sidebar.md",
        world: Some(pairing_waiting),
        frame: Frame::Dialog,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("hub-switcher"),
            Step::ClickItem("Pair a device with this Mac…"),
            Step::WaitFor("pairing-link-text"),
            Step::MoveAway,
        ],
    },
    Screen {
        name: "pair-device-paired",
        spec: "sidebar.md",
        world: Some(pairing_connected),
        frame: Frame::Dialog,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("hub-switcher"),
            Step::ClickItem("Pair a device with this Mac…"),
            Step::WaitFor("paired-device-card"),
            Step::MoveAway,
        ],
    },
];

fn with_saved_hub() -> World {
    let mut world = conversation();
    world.saved_hubs = vec![("Mac Studio", "100.64.0.7:7867")];
    world
}

fn pairing_waiting() -> World {
    let mut world = conversation();
    world.pairing = Some(PairingWorld { paired_device: None });
    world
}

fn pairing_connected() -> World {
    let mut world = conversation();
    world.pairing = Some(PairingWorld { paired_device: Some("Ada’s MacBook Air") });
    world
}

/// The conversation, with its workspaces' lines, whose chat's cache is about to expire, nine minutes from now.
fn expiring() -> World {
    let mut world = with_lines(conversation());
    let chat = world.sessions.iter().rfind(|session| session.last_seq > Seq::default()).unwrap().id.clone();
    world.cached(&chat, 51);
    world
}
