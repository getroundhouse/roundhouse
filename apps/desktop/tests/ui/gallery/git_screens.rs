//! Git's screens in Settings, each of a state its rules in `settings.md` say: the accounts and logins in each state,
//! connecting one, signing in, a row's menu, and a hub that keeps no GitHub accounts.

#![cfg(test)]

use protocol::model::GithubSignInStatus;

use super::frame::Frame;
use super::screens::FOCUS;
use super::steps::{Screen, Step};
use super::worlds::{
    ada, conversation, github_accounts, github_default_ended, github_gh_default, github_gh_logged_out,
    github_gh_offered, github_gh_other_user, github_gh_refused, github_gh_unread, github_signing_in,
    github_signing_in_to, no_github_accounts,
};
use client_core::fake_hub::World;

/// Shows Git, once Settings is open.
const GIT: Step = Step::Click("settings-page-git");

/// A token pasted into the panel, which is plainly made up.
const TOKEN: &str = "github_pat_made_up";

const fn git(name: &'static str, world: fn() -> World, steps: &'static [Step]) -> Screen {
    Screen { name, spec: "settings.md", world: Some(world), frame: Frame::Layout, steps }
}

pub(crate) const SCREENS: &[Screen] = &[
    git("settings-git", github_accounts, &[Step::Select(FOCUS), Step::OpenSettings, GIT]),
    git("settings-git-none", no_github_accounts, &[Step::Select(FOCUS), Step::OpenSettings, GIT]),
    git("settings-git-default-ended", github_default_ended, &[Step::Select(FOCUS), Step::OpenSettings, GIT]),
    git("settings-git-older-hub", conversation, &[Step::Select(FOCUS), Step::OpenSettings, GIT]),
    git(
        "settings-git-menu",
        github_accounts,
        &[Step::Select(FOCUS), Step::OpenSettings, GIT, Step::Click("git-more-github-0002")],
    ),
    git(
        "settings-git-choosing",
        github_accounts,
        &[Step::Select(FOCUS), Step::OpenSettings, GIT, Step::Click("git-connect")],
    ),
    git(
        "settings-git-choosing-gh",
        github_gh_offered,
        &[Step::Select(FOCUS), Step::OpenSettings, GIT, Step::Click("git-connect"), Step::WaitFor("git-use-gh")],
    ),
    git(
        "settings-git-gh-refused",
        github_gh_refused,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            GIT,
            Step::Click("git-connect"),
            Step::Click("git-use-gh"),
            Step::WaitFor("git-refused"),
        ],
    ),
    git("settings-git-gh", github_gh_default, &[Step::Select(FOCUS), Step::OpenSettings, GIT]),
    git("settings-git-gh-logged-out", github_gh_logged_out, &[Step::Select(FOCUS), Step::OpenSettings, GIT]),
    git("settings-git-gh-other-user", github_gh_other_user, &[Step::Select(FOCUS), Step::OpenSettings, GIT]),
    git("settings-git-gh-unread", github_gh_unread, &[Step::Select(FOCUS), Step::OpenSettings, GIT]),
    git(
        "settings-git-token",
        github_accounts,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            GIT,
            Step::Click("git-connect"),
            Step::Click("git-use-token"),
            Step::Type(TOKEN),
        ],
    ),
    git(
        "settings-git-signing-in",
        github_signing_in,
        &[Step::Select(FOCUS), Step::OpenSettings, GIT, Step::WaitFor("git-code")],
    ),
    git(
        "settings-git-sign-in-refused",
        refused,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            GIT,
            Step::Click("git-connect"),
            Step::Click("git-sign-in"),
            Step::WaitFor("git-sign-in-again"),
        ],
    ),
    git(
        "settings-git-signed-in",
        approved,
        &[
            Step::Select(FOCUS),
            Step::OpenSettings,
            GIT,
            Step::Click("git-connect"),
            Step::Click("git-sign-in"),
            Step::WaitFor("git-close"),
        ],
    ),
];

fn refused() -> World {
    github_signing_in_to(GithubSignInStatus::Refused)
}

/// Approved as `ada`, the default.
fn approved() -> World {
    github_signing_in_to(GithubSignInStatus::Approved { account: ada().id })
}
