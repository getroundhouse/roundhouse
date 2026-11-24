//! What the page's unit tests are drawn from: a moment held still, logins, accounts, the hub's records of them, and
//! sign-in panels, each plainly made up.

use client_core::records::{HubRecords, Snapshot};
use protocol::fixtures::{github_account, github_sign_in, observed};
use protocol::model::{
    GithubAccount, GithubAccountId, GithubLoginKind, GithubSettings, GithubSignIn, GithubSignInStatus, HeldGithubLogin,
    HubSettings, Observation, Records, Rev,
};

use super::super::sign_in::SignInPanel;

/// Sunday 1 November 2026, at 3 PM in UTC, which held still is the local time zone.
pub(super) const NOW_MS: i64 = 1_793_545_200_000;
pub(super) const HOUR_MS: i64 = 60 * 60_000;
/// Feb 3, 2027, at 3 PM in UTC.
pub(super) const FEB_3_MS: i64 = 1_801_666_800_000;
/// Oct 29, 2026, at 3 PM in UTC.
pub(super) const OCT_29_MS: i64 = 1_793_286_000_000;

pub(super) fn held(how: GithubLoginKind, ends_at_ms: Option<i64>, needs_sign_in: bool) -> HeldGithubLogin {
    HeldGithubLogin { how, connected_at_ms: 0, ends_at_ms, needs_sign_in }
}

/// The account `id`, whose login the hub holds unless it's disconnected, and what GitHub calls its user, `name`.
pub(super) fn named(id: &str, name: &str, disconnected: bool) -> (GithubAccount, Observation) {
    let account = github_account(id);
    let account = if disconnected { GithubAccount { held_login: None, ..account } } else { account };
    (account, observed::username(id, name))
}

/// The hub's records: `accounts`, with what GitHub calls each's user, `sign_ins`, and settings that make `default` the
/// default.
pub(super) fn records(
    accounts: Vec<(GithubAccount, Observation)>,
    sign_ins: Vec<GithubSignIn>,
    default: Option<&str>,
) -> Snapshot {
    let (github_accounts, observations) = accounts.into_iter().unzip();
    listed(Records { github_accounts, github_sign_ins: sign_ins, observations, ..Records::default() }, default)
}

/// `records`, with settings that make `default` the default, as the hub lists them.
pub(super) fn listed(records: Records, default: Option<&str>) -> Snapshot {
    let settings = HubSettings {
        keep_awake: true,
        allow_asked_in_full_access: None,
        account_defaults: None,
        github: Some(GithubSettings {
            account: default.map(GithubAccountId::from),
            sent: None,
            left_to_machine: Vec::new(),
            set_up: Vec::new(),
        }),
        rev: Rev::default(),
    };
    let mut held = HubRecords::default();
    let listed = Records { hub_settings: Some(settings), rev: Rev::new(1), ..records };
    held.apply_list(listed);
    held.snapshot()
}

pub(super) fn panel(id: &str, starting: bool, not_started: Option<&str>) -> SignInPanel {
    SignInPanel { id: id.into(), starting, not_started: not_started.map(str::to_string), again: None }
}

pub(super) fn ended(id: &str, status: GithubSignInStatus) -> GithubSignIn {
    GithubSignIn { status, ..github_sign_in(id) }
}
