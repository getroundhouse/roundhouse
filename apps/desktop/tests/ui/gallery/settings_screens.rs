//! More of Settings' screens, each of a state its rules in `settings.md` say, apart from the rest in `screens.rs` only
//! to keep each list a readable length.

#![cfg(test)]

use super::frame::Frame;
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::world::{LoginSupport, World};
use super::worlds::{accounts, conversation, logins_unkept, none_found, own_hub};

/// What puts the cursor in Settings' search.
const FIND: &str = "secondary-f";

/// Shows Accounts, once Settings is open.
const ACCOUNTS: Step = Step::Click("settings-page-accounts");

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
        name: "settings-accounts",
        spec: "settings.md",
        world: Some(accounts),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, ACCOUNTS],
    },
    Screen {
        name: "settings-accounts-default-picker",
        spec: "settings.md",
        world: Some(accounts),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, ACCOUNTS, Step::Click("account-default-claude")],
    },
    Screen {
        name: "settings-accounts-default-ended",
        spec: "settings.md",
        world: Some(default_ended),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, ACCOUNTS],
    },
    Screen {
        name: "settings-accounts-no-defaults",
        spec: "settings.md",
        world: Some(defaults_unkept),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, ACCOUNTS],
    },
    Screen {
        name: "settings-accounts-none-found",
        spec: "settings.md",
        world: Some(none_found),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, ACCOUNTS],
    },
    Screen {
        name: "settings-accounts-connecting",
        spec: "settings.md",
        world: Some(none_found),
        frame: Frame::Layout,
        steps: &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            ACCOUNTS,
            Step::Click("accounts-connect"),
            Step::Type(TOKEN),
            Step::Click("accounts-name"),
            Step::Type("Build machines"),
        ],
    },
    Screen {
        name: "settings-accounts-renewing",
        spec: "settings.md",
        world: Some(accounts),
        frame: Frame::Layout,
        steps: &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            ACCOUNTS,
            Step::Click("account-more-claude-login-0002"),
            Step::ClickItem("Renew"),
            Step::Type(TOKEN),
        ],
    },
    Screen {
        name: "settings-accounts-older-hub",
        spec: "settings.md",
        world: Some(logins_unkept),
        frame: Frame::Layout,
        steps: &[Step::Select(FOCUS), Step::OpenSettings, ACCOUNTS, Step::WaitFor("accounts-older-hub")],
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
