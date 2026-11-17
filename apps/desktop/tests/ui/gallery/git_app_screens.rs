//! Git's screens of an app of your own, each of a state its rules in `settings.md` say: each step of the panel that
//! makes one, as far as a screen can take it, since GitHub never sends anyone back here; why the hub couldn't look for
//! where it's installed; and its rows, installed as the default, as the default whose token has ended, and installed
//! nowhere, with their menus.

#![cfg(test)]

use super::frame::Frame;
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::worlds::{
    github_accounts, github_app_installed, github_app_not_installed, github_app_not_looked, github_app_renewing,
};
use client_core::fake_hub::World;

/// Shows Git, once Settings is open.
const GIT: Step = Step::Click("settings-page-git");

/// Opens Connect account's panel.
const CONNECT: Step = Step::Click("git-connect");
/// Opens the panel for making an app, at its first step, from Connect account's.
const MAKE: Step = Step::Click("git-make-app");

/// Opens the menu of the app installed nowhere.
const MENU: Step = Step::Click("git-more-app-9");
/// Opens the panel at its last step, from the menu of the app installed nowhere.
const INSTALL: Step = Step::ClickItem("Install on GitHub");

/// Rests the pointer on what the panel says under its title, which has no hover of its own, so the picture shows the
/// step rather than the button that was pressed. Settings has no sidebar version to move it to.
const AWAY: Step = Step::Point("git-panel-says");

const fn git(name: &'static str, world: fn() -> World, steps: &'static [Step]) -> Screen {
    Screen { name, spec: "settings.md", world: Some(world), frame: Frame::Layout, steps }
}

pub(crate) const SCREENS: &[Screen] = &[
    git("settings-git-app-choosing", github_accounts, &[Step::Select(FOCUS), Step::OpenSettings, GIT, CONNECT, MAKE]),
    git(
        "settings-git-app-organization",
        github_accounts,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            GIT,
            CONNECT,
            MAKE,
            Step::Click("git-app-under-organization"),
            Step::Type("adalabs"),
        ],
    ),
    git(
        "settings-git-app-on-github",
        github_accounts,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            GIT,
            CONNECT,
            MAKE,
            Step::Click("git-make-on-github"),
            Step::WaitFor("git-open-github"),
            AWAY,
        ],
    ),
    git(
        "settings-git-app-installing",
        github_app_not_installed,
        &[Step::Select(FOCUS), Step::OpenSettings, GIT, MENU, INSTALL],
    ),
    git(
        "settings-git-app-not-looked",
        github_app_not_looked,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            GIT,
            MENU,
            INSTALL,
            Step::Click("git-check-again"),
            Step::WaitFor("git-refused"),
            AWAY,
        ],
    ),
    git("settings-git-app", github_app_installed, &[Step::Select(FOCUS), Step::OpenSettings, GIT]),
    git("settings-git-app-renewing", github_app_renewing, &[Step::Select(FOCUS), Step::OpenSettings, GIT]),
    git(
        "settings-git-app-menu",
        github_app_installed,
        &[Step::Select(FOCUS), Step::OpenSettings, GIT, Step::Click("git-more-github-0004")],
    ),
    git(
        "settings-git-app-not-installed",
        github_app_not_installed,
        &[Step::Select(FOCUS), Step::OpenSettings, GIT, MENU],
    ),
];
