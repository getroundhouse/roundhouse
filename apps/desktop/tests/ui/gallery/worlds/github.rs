//! The GitHub accounts agents push as, as Git in Settings lists them: the accounts the person gave the hub, by signing
//! in with GitHub and with tokens, in each state one can be in, and the `gh` login found on each provider's machines.
//! Their tokens are the hub's, so none is here.

use protocol::fixtures::{found_github_login, github_account, github_sign_in};
use protocol::model::{
    FoundGithubLogin, GhLogin, GhLoginWorks, GithubAccount, GithubLoginKind, GithubSettings, GithubSignInStatus,
    GithubUser, GithubUserId, HeldGithubLogin, SentCredential, UnusableLogin,
};

use super::conversation;
use client_core::fake_hub::{HOUR_MS, NOW_MS, World};

const DAY_MS: i64 = 24 * HOUR_MS;
/// Feb 3, 2027, at 3 PM in UTC, when a fine-grained token made for 90 days ends.
const FEB_3_MS: i64 = 1_801_666_800_000;
/// Oct 29, 2026, at 3 PM in UTC, three days before the pictures are taken.
const OCT_29_MS: i64 = 1_793_286_000_000;

/// The conversation, on a hub with three GitHub accounts: `ada`, signed in, the default; `ada_northwind`, a token that
/// ends in February; and `grace`, whose token was disconnected. Found has this Mac's `gh`, logged in as `ada`, which
/// GitHub took, and a server's, with no `gh`.
pub(in crate::gallery) fn github_accounts() -> World {
    let mut world = conversation();
    world.github = Some(sending(&ada()));
    world.github_accounts = vec![ada(), northwind(FEB_3_MS), grace()];
    world.found_github_logins = vec![found_github_login("local"), not_installed("incus")];
    world
}

/// The conversation, on a hub with no GitHub account, so each machine's own login is the default, with this Mac's `gh`
/// logged in as `ada`, and a server's logged in to no one.
pub(in crate::gallery) fn no_github_accounts() -> World {
    let mut world = conversation();
    world.github = Some(GithubSettings { account: None, sent: Some(SentCredential::MachineOwn) });
    world.found_github_logins = vec![found_github_login("local"), logged_out("incus")];
    world
}

/// The conversation, on a hub whose default, `ada_northwind`'s token, ended three days ago, so agents can't push, with
/// `ada` signed in to choose instead. This Mac's `gh` is logged in with a token GitHub refused.
pub(in crate::gallery) fn github_default_ended() -> World {
    let mut world = conversation();
    let reason = "The token of Roundhouse's default GitHub account, ada_northwind, has ended, so this workspace's \
                  agents push to GitHub as nobody. Renew it in Roundhouse's Settings, or choose another default there.";
    world.github = Some(blocking(&northwind(OCT_29_MS), UnusableLogin::Ended, reason));
    world.github_accounts = vec![ada(), northwind(OCT_29_MS)];
    let refused = GhLogin::LoggedIn { username: "ada".into(), works: GhLoginWorks::Refused };
    world.found_github_logins = vec![FoundGithubLogin { login: refused, ..found_github_login("local") }];
    world
}

/// [`github_accounts`], with a sign-in that waits for its code, `WDJB-MJHT`, to be typed on GitHub, for 15 minutes.
pub(in crate::gallery) fn github_signing_in() -> World {
    let mut world = github_accounts();
    let mut waiting = github_sign_in("github-sign-in-0001");
    if let GithubSignInStatus::Waiting { code_ends_at_ms, .. } = &mut waiting.status {
        *code_ends_at_ms = NOW_MS + 15 * 60_000;
    }
    world.github_sign_ins = vec![waiting];
    world
}

/// [`github_accounts`], whose hub answers a sign-in as `status`.
pub(in crate::gallery) fn github_signing_in_to(status: GithubSignInStatus) -> World {
    let mut world = github_accounts();
    world.sign_in_answer = Some(status);
    world
}

/// The settings of a hub whose default is `account`, whose login workers are sent.
pub(in crate::gallery) fn sending(account: &GithubAccount) -> GithubSettings {
    let sent = SentCredential::Account { account: account.id.clone() };
    GithubSettings { account: Some(account.id.clone()), sent: Some(sent) }
}

/// The settings of a hub whose default is `account`, which blocks every worker, for the reason `why`, saying
/// `reason`, as the hub words it.
pub(in crate::gallery) fn blocking(account: &GithubAccount, why: UnusableLogin, reason: &str) -> GithubSettings {
    let sent = SentCredential::Blocked { account: account.id.clone(), why, reason: reason.to_string() };
    GithubSettings { account: Some(account.id.clone()), sent: Some(sent) }
}

/// The account `ada` signed in as, three weeks ago, whose token the hub renews by itself.
pub(in crate::gallery) fn ada() -> GithubAccount {
    held(
        "github-0001",
        1,
        "ada",
        HeldGithubLogin {
            how: GithubLoginKind::SignIn,
            connected_at_ms: NOW_MS - 21 * DAY_MS,
            ends_at_ms: Some(NOW_MS + 6 * HOUR_MS),
            needs_sign_in: false,
        },
    )
}

/// `ada_northwind`, a token made for 90 days, which ends at `ends_at_ms`.
fn northwind(ends_at_ms: i64) -> GithubAccount {
    held(
        "github-0002",
        2,
        "ada_northwind",
        HeldGithubLogin {
            how: GithubLoginKind::Token,
            connected_at_ms: ends_at_ms - 90 * DAY_MS,
            ends_at_ms: Some(ends_at_ms),
            needs_sign_in: false,
        },
    )
}

/// `grace`, whose token the hub was told to forget.
fn grace() -> GithubAccount {
    GithubAccount { held_login: None, ..held("github-0003", 3, "grace", ada().held_login.unwrap()) }
}

fn held(id: &str, user: u64, name: &str, held: HeldGithubLogin) -> GithubAccount {
    GithubAccount {
        user: GithubUser { id: GithubUserId::new(user), name: name.into() },
        held_login: Some(held),
        ..github_account(id)
    }
}

fn not_installed(provider: &str) -> FoundGithubLogin {
    FoundGithubLogin { login: GhLogin::NotInstalled, ..found_github_login(provider) }
}

fn logged_out(provider: &str) -> FoundGithubLogin {
    FoundGithubLogin { login: GhLogin::LoggedOut, ..found_github_login(provider) }
}
