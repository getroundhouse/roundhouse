//! The GitHub accounts agents push as, as Git in Settings lists them: the accounts the person gave the hub, by signing
//! in with GitHub and with tokens, in each state one can be in. Their tokens are the hub's, so none is here.

use protocol::fixtures::{github_account, github_sign_in, github_user, observed};
use protocol::model::{
    GithubAccount, GithubLoginKind, GithubSettings, GithubSignInStatus, HeldGithubLogin, Observation, Observed, Rev,
    SentCredential, UnusableLogin,
};

use super::conversation;
use client_core::fake_hub::{HOUR_MS, NOW_MS, World};

const DAY_MS: i64 = 24 * HOUR_MS;
/// Feb 3, 2027, at 3 PM in UTC, when a fine-grained token made for 90 days ends.
pub(super) const FEB_3_MS: i64 = 1_801_666_800_000;
/// Oct 29, 2026, at 3 PM in UTC, three days before the pictures are taken.
const OCT_29_MS: i64 = 1_793_286_000_000;

/// A GitHub account's record, and what GitHub calls its user, and, for an installation of an app of your own, the user
/// or organization it's on.
pub(in crate::gallery) struct Named {
    pub(in crate::gallery) account: GithubAccount,
    pub(in crate::gallery) username: &'static str,
    pub(in crate::gallery) on: Option<&'static str>,
}

/// Has the world's hub hold `accounts`, in place of any it held, with what GitHub calls each.
pub(in crate::gallery) fn hold(world: &mut World, accounts: Vec<Named>) {
    world.github_accounts.clear();
    for account in accounts {
        add(world, account);
    }
}

/// Has the world's hub hold `account` too, with what GitHub calls it.
pub(in crate::gallery) fn add(world: &mut World, named: Named) {
    let Named { account, username, on } = named;
    world.observe(observed::username(account.id.clone(), username));
    if let Some(on) = on {
        world.observe(observed::installed_on(account.id.clone(), on));
    }
    world.github_accounts.push(account);
}

/// The conversation, on a hub with three GitHub accounts: `ada`, signed in, the default; `ada_northwind`, a token that
/// ends in February; and `grace`, whose token ended three days ago.
pub(in crate::gallery) fn github_accounts() -> World {
    let mut world = conversation();
    world.github = Some(sending(&ada().account));
    hold(&mut world, vec![ada(), northwind(FEB_3_MS), grace()]);
    world
}

/// The same, before the hub has asked GitHub what `grace`'s user is called now, as after an update emptied the cache,
/// so her row is named by her user's number and listed after the named ones.
pub(in crate::gallery) fn github_unnamed() -> World {
    let mut world = github_accounts();
    let username = Observed::NotKnown;
    world.observe(Observation::GithubUsername { account: grace().account.id, username, rev: Rev::new(1) });
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

/// The conversation, on a hub whose default was disconnected, which leaves none, with `ada` signed in and
/// `ada_northwind`'s token to choose from, so it reads nothing of GitHub and blocks the workers of the providers it sets
/// up, as the hub words why.
pub(in crate::gallery) fn github_no_default() -> World {
    let mut world = conversation();
    let reason = "Roundhouse has no default GitHub account. Until one is chosen, Roundhouse can't read pull requests, \
                  and agents on the providers it sets up can't push. Make one the default in Roundhouse's Settings, or \
                  connect another.";
    let sent = SentCredential::NoAccount { reason: reason.to_string() };
    world.github =
        Some(GithubSettings { account: None, sent: Some(sent), left_to_machine: Vec::new(), set_up: Vec::new() });
    hold(&mut world, vec![ada(), northwind(FEB_3_MS)]);
    world
}

/// The conversation, on a hub whose default, `ada_northwind`'s token, ended three days ago, so agents can't push, with
/// `ada` signed in to choose instead.
pub(in crate::gallery) fn github_default_ended() -> World {
    let mut world = conversation();
    let reason = "The token of ada_northwind, the default account, has ended. Until it's renewed, Roundhouse can't \
                  read pull requests, and agents on the providers it sets up can't push. Renew it, or choose another \
                  default.";
    world.github = Some(blocking(&northwind(OCT_29_MS).account, UnusableLogin::Ended, reason));
    hold(&mut world, vec![ada(), northwind(OCT_29_MS)]);
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
pub(in crate::gallery) fn ada() -> Named {
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
pub(super) fn northwind(ends_at_ms: i64) -> Named {
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

/// `grace`, a token made for 90 days, which ended three days ago.
fn grace() -> Named {
    let ended = HeldGithubLogin {
        how: GithubLoginKind::Token,
        connected_at_ms: OCT_29_MS - 90 * DAY_MS,
        ends_at_ms: Some(OCT_29_MS),
        needs_sign_in: false,
    };
    held("github-0003", 3, "grace", ended)
}

/// The account `id`, of GitHub's user `user`, called `name`, whose login the hub holds as `held` says.
fn held(id: &str, user: u64, name: &'static str, held: HeldGithubLogin) -> Named {
    let account = GithubAccount { user: github_user(user), held_login: Some(held), ..github_account(id) };
    Named { account, username: name, on: None }
}
