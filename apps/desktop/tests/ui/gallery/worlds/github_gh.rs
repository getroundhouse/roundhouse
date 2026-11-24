//! The `gh` login on the hub's machine as a GitHub account: offered in Connect account's panel, refused as it's
//! connected, added as the default, and the default when `gh` there is logged out, or logged in as someone else, as the
//! hub words why agents can't push, and while the hub hasn't read it lately. Its token is `gh`'s, so none is here.

use protocol::client::{ApiError, ErrorCode};
use protocol::fixtures::{github_account, github_user};
use protocol::model::{GhLogin, GhLoginWorks, GithubAccount, GithubLoginKind, HeldGithubLogin, UnusableLogin};

use super::conversation;
use super::github::{FEB_3_MS, Named, blocking, github_accounts, hold, northwind, sending};
use client_core::fake_hub::{HOUR_MS, NOW_MS, World};

/// [`github_accounts`], whose hub's `gh` is logged in as `ada`, with a token GitHub takes, so Connect account offers it.
pub(in crate::gallery) fn github_gh_offered() -> World {
    let mut world = github_accounts();
    world.hub_gh_login = Some(GhLogin::LoggedIn { username: "ada".into(), works: GhLoginWorks::Yes });
    world
}

/// [`github_gh_offered`], whose hub refuses to use `gh`'s login, as GitHub refused its token as it was connected.
pub(in crate::gallery) fn github_gh_refused() -> World {
    let mut world = github_gh_offered();
    let why = "GitHub refused the token of gh on the Mac Roundhouse runs on. Run gh auth login, then try again.";
    world.gh_connect_refused = Some(ApiError::new(ErrorCode::InvalidRequest, why));
    world
}

/// The conversation, on a hub whose default GitHub account is `gh`'s login on its machine, as `ada`, beside a token.
pub(in crate::gallery) fn github_gh_default() -> World {
    let mut world = conversation();
    world.github = Some(sending(&gh_ada().account));
    hold(&mut world, vec![gh_ada(), northwind(FEB_3_MS)]);
    world
}

/// [`github_gh_default`], once `gh` there has logged out, so agents can't push.
pub(in crate::gallery) fn github_gh_logged_out() -> World {
    let reason = "ada, the default account, is gh's login, and gh on the Mac Roundhouse runs on is logged out. Until \
                  gh is logged in as ada again, Roundhouse can't read pull requests, and agents on the providers it \
                  sets up can't push. Run gh auth login, or choose another default.";
    blocked_by_gh(UnusableLogin::GhLoggedOut, reason)
}

/// [`github_gh_default`], once `gh` there has switched to `grace`, so agents can't push.
pub(in crate::gallery) fn github_gh_other_user() -> World {
    let reason = "ada, the default account, is gh's login on the Mac Roundhouse runs on, which is grace now. Until \
                  it's ada again, Roundhouse can't read pull requests, and agents on the providers it sets up can't \
                  push. Run gh auth switch --user ada, connect grace, or choose another default.";
    blocked_by_gh(UnusableLogin::GhOtherUser, reason)
}

/// [`github_gh_default`], while the hub hasn't read `gh` there lately, as when `gh` doesn't answer.
pub(in crate::gallery) fn github_gh_unread() -> World {
    let reason = "ada, the default account, is gh's login on the Mac Roundhouse runs on, which Roundhouse couldn't \
                  read lately (couldn't tell who gh is logged in to github.com as: gh took more than 15s). Until it \
                  can, Roundhouse can't read pull requests, and agents on the providers it sets up can't push. It asks \
                  gh again every minute.";
    blocked_by_gh(UnusableLogin::GhUnread, reason)
}

fn blocked_by_gh(why: UnusableLogin, reason: &str) -> World {
    let mut world = github_gh_default();
    world.github = Some(blocking(&gh_ada().account, why, reason));
    world
}

/// `ada`, as the login `gh` has on the hub's machine, connected an hour ago.
fn gh_ada() -> Named {
    let held = HeldGithubLogin {
        how: GithubLoginKind::Gh,
        connected_at_ms: NOW_MS - HOUR_MS,
        ends_at_ms: None,
        needs_sign_in: false,
    };
    let account = GithubAccount { user: github_user(1), held_login: Some(held), ..github_account("github-0004") };
    Named { account, username: "ada", on: None }
}
