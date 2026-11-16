//! More of Settings' screens, each of a state its rules in `settings.md` say, apart from the rest in `screens.rs` only
//! to keep each list a readable length.

#![cfg(test)]

use desktop::dictating::{DELETE, RETRY, STATUS};
use look::Region;

use super::frame::Frame;
use super::screens::FOCUS;
use super::steps::{Screen, Step, Target};
use super::world::{LoginSupport, World};
use super::worlds::{accounts, conversation, logins_unkept, none_found, own_hub};

/// What puts the cursor in Settings' search.
const FIND: &str = "secondary-f";

/// Shows Agents, once Settings is open.
const AGENTS: Step = Step::Click("settings-page-agents");

/// Opens Dictation's choices, once Settings is open, for [`ON`] or [`OFF`] to choose from.
const DICTATION: Step = Step::Click("settings-dictation");
const ON: Step = Step::ClickItem("On");
const OFF: Step = Step::ClickItem("Off");

/// A token pasted into the panel, which is plainly made up.
const TOKEN: &str = "a-made-up-token";

pub(crate) const SCREENS: &[Screen] = &[
    Screen {
        name: "settings-search",
        spec: "settings.md",
        world: Some(conversation),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, Step::Press(FIND), Step::Type("sound")],
    },
    Screen {
        name: "settings-nothing-found",
        spec: "settings.md",
        world: Some(conversation),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, Step::Press(FIND), Step::Type("zebra")],
    },
    Screen {
        name: "settings-agents",
        spec: "settings.md",
        world: Some(accounts),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, AGENTS],
    },
    Screen {
        name: "settings-agents-default-picker",
        spec: "settings.md",
        world: Some(accounts),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, AGENTS, Step::Click("account-default-claude")],
    },
    Screen {
        name: "settings-agents-default-ended",
        spec: "settings.md",
        world: Some(default_ended),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, AGENTS],
    },
    Screen {
        name: "settings-agents-no-defaults",
        spec: "settings.md",
        world: Some(defaults_unkept),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, AGENTS],
    },
    Screen {
        name: "settings-agents-none-found",
        spec: "settings.md",
        world: Some(none_found),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, AGENTS],
    },
    Screen {
        name: "settings-agents-connecting",
        spec: "settings.md",
        world: Some(none_found),
        frame: Frame::Layout,
        steps: &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            AGENTS,
            Step::Click("accounts-connect"),
            Step::Type(TOKEN),
            Step::Click("accounts-name"),
            Step::Type("Build machines"),
        ],
    },
    Screen {
        name: "settings-agents-menu",
        spec: "settings.md",
        world: Some(accounts),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, AGENTS, Step::Click("account-more-claude-login-0002")],
    },
    Screen {
        name: "settings-agents-card",
        spec: "settings.md",
        world: Some(accounts),
        frame: Frame::Layout,
        steps: &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            AGENTS,
            Step::Hover { on: Target::Id("account-claude-login-0001"), shows: Region::AccountCard.id() },
        ],
    },
    Screen {
        name: "settings-agents-renewing",
        spec: "settings.md",
        world: Some(accounts),
        frame: Frame::Layout,
        steps: &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            AGENTS,
            Step::Click("account-more-claude-login-0002"),
            Step::ClickItem("Renew"),
            Step::Type(TOKEN),
        ],
    },
    Screen {
        name: "settings-agents-older-hub",
        spec: "settings.md",
        world: Some(logins_unkept),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, AGENTS, Step::WaitFor("accounts-older-hub")],
    },
    Screen {
        name: "settings-dictation-downloading",
        spec: "settings.md",
        world: Some(conversation),
        frame: Frame::Layout,
        steps: &[
            Step::Select(FOCUS),
            Step::StallDictationDownload,
            Step::OpenSettings,
            DICTATION,
            ON,
            Step::WaitFor(STATUS),
        ],
    },
    Screen {
        name: "settings-dictation-kept",
        spec: "settings.md",
        world: Some(conversation),
        frame: Frame::Layout,
        steps: &[
            Step::Select(FOCUS),
            Step::DictationModelHere,
            Step::OpenSettings,
            DICTATION,
            OFF,
            Step::WaitFor(DELETE),
        ],
    },
    Screen {
        name: "settings-dictation-ready",
        spec: "settings.md",
        world: Some(conversation),
        frame: Frame::Layout,
        steps: &[
            Step::Select(FOCUS),
            Step::DictationModelHere,
            Step::OpenSettings,
            DICTATION,
            ON,
            Step::WaitFor(DELETE),
        ],
    },
    Screen {
        name: "settings-dictation-failed",
        spec: "settings.md",
        world: Some(conversation),
        frame: Frame::Layout,
        steps: &[
            Step::Select(FOCUS),
            Step::FailDictationDownload,
            Step::OpenSettings,
            DICTATION,
            ON,
            Step::WaitFor(RETRY),
        ],
    },
    Screen {
        name: "settings-older-hub",
        spec: "settings.md",
        world: Some(settings_unkept),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings],
    },
];

/// An older hub, from before hubs kept settings.
fn settings_unkept() -> World {
    let mut world = own_hub();
    world.keeps_no_settings = true;
    world
}

fn default_ended() -> World {
    let mut world = accounts();
    world.account_defaults[0].account = "claude-login-0003".into();
    world
}

fn defaults_unkept() -> World {
    let mut world = accounts();
    world.logins = LoginSupport::Accounts;
    world
}
