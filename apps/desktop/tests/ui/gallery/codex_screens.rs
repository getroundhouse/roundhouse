use super::frame::{Frame, TABS_MENU};
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::world::World;

fn offered() -> World {
    let mut world = super::worlds::titled();
    world.agents = Some(agents::descriptions());
    world
}

pub(super) const SCREENS: &[Screen] = &[
    Screen {
        name: "new-workspace-agents",
        spec: "workspaces.md",
        world: Some(offered),
        frame: Frame::Dialog,
        steps: &[Step::Select(FOCUS), Step::Click("new-in-repo-1"), Step::Click("new-workspace-agent")],
    },
    Screen {
        name: "codex-models",
        spec: "chats.md",
        world: Some(offered),
        frame: Frame::Dialog,
        steps: &[
            Step::Select(FOCUS),
            Step::Click("new-in-repo-1"),
            Step::Click("new-workspace-agent"),
            Step::ClickItem("Codex"),
            Step::Click("new-workspace-model"),
        ],
    },
    Screen {
        name: "new-chat-agents",
        spec: "chats.md",
        world: Some(offered),
        frame: TABS_MENU,
        steps: &[Step::Select(FOCUS), Step::Click("new-chat")],
    },
];
