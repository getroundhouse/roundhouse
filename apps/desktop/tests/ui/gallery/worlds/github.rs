//! The GitHub accounts agents push as, as Git in Settings lists them: the accounts the person gave the hub, by signing
//! in with GitHub and with tokens, in each state one can be in. Their tokens are the hub's, so none is here.

use protocol::fixtures::{github_account, github_sign_in};
use protocol::model::{
    GithubAccount, GithubLoginKind, GithubSettings, GithubSignInStatus, GithubUser, GithubUserId, HeldGithubLogin,
    SentCredential, UnusableLogin,
};

use super::conversation;
use client_core::fake_hub::{HOUR_MS, NOW_MS, World};

const DAY_MS: i64 = 24 * HOUR_MS;
/// Feb 3, 2027, at 3 PM in UTC, when a fine-grained token made for 90 days ends.
pub(super) const FEB_3_MS: i64 = 1_801_666_800_000;
/// Oct 29, 2026, at 3 PM in UTC, three days before the pictures are taken.
const OCT_29_MS: i64 = 1_793_286_000_000;

/// The conversation, on a hub with three GitHub accounts: `ada`, signed in, the default; `ada_northwind`, a token that
/// ends in February; and `grace`, whose token was disconnected.
pub(in crate::gallery) fn github_accounts() -> World {
    let mut world = conversation();
    world.github = Some(sending(&ada()));
    world.github_accounts = vec![ada(), northwind(FEB_3_MS), grace()];
    world
}

/// The conversation, on a hub with no GitHub account, so it reads nothing of GitHub, and blocks the workers of the
/// providers it sets up, as the hub words why.
pub(in crate::gallery) fn no_github_accounts() -> World {
    let mut world = conversation();
    let reason = "Roundhouse has no GitHub account. Until one is connected, Roundhouse can't read pull requests, and \
                  agents on the providers it sets up can't push. Connect one in Roundhouse's Settings, such as gh's \
                  login on the machine Roundhouse runs on.";
    let sent = SentCredential::NoAccount { reason: reason.to_string() };
    world.github =
        Some(GithubSettings { account: None, sent: Some(sent), left_to_machine: Vec::new(), set_up: Vec::new() });
    world
}

/// The conversation, on a hub whose default, `ada_northwind`'s token, ended three days ago, so agents can't push, with
/// `ada` signed in to choose instead.
pub(in crate::gallery) fn github_default_ended() -> World {
    let mut world = conversation();
    let reason = "The token of ada_northwind, the default account, has ended. Until it's renewed, Roundhouse can't \
                  read pull requests, and agents on the providers it sets up can't push. Renew it, or choose another \
                  default.";
    world.github = Some(blocking(&northwind(OCT_29_MS), UnusableLogin::Ended, reason));
    world.github_accounts = vec![ada(), northwind(OCT_29_MS)];
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
    GithubSettings {
        account: Some(account.id.clone()),
        sent: Some(sent),
        left_to_machine: Vec::new(),
        set_up: Vec::new(),
    }
}

/// The settings of a hub whose default is `account`, which blocks every worker, for the reason `why`, saying
/// `reason`, as the hub words it.
pub(in crate::gallery) fn blocking(account: &GithubAccount, why: UnusableLogin, reason: &str) -> GithubSettings {
    let sent = SentCredential::Blocked { account: account.id.clone(), why, reason: reason.to_string() };
    GithubSettings {
        account: Some(account.id.clone()),
        sent: Some(sent),
        left_to_machine: Vec::new(),
        set_up: Vec::new(),
    }
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
pub(super) fn northwind(ends_at_ms: i64) -> GithubAccount {
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
