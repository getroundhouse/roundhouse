//! Which account Automatic puts a chat on: of the logins added for its kind, the one with the most of its week to
//! spare, as `AccountSetAutomatic` says. What's left of a week is gone once it starts again, so an account whose week
//! starts again sooner has more to spare for each hour, and the chats already at work on an account share what it has.

use protocol::model::{Account, AccountId, AgentKind, Reserve};

/// The week, in hours, which an account whose week no agent has heard is taken to have all of.
const WEEK_HOURS: f64 = 7. * 24.;

/// The fewest hours a week is taken to have left, so one about to start again isn't taken to have all but endless
/// usage to spare for each hour, which would put every chat on it in its last minutes.
const FEWEST_HOURS: f64 = 1.;

const HOUR_MS: f64 = 60. * 60. * 1000.;

/// An account Automatic could put a chat on, and how much of its week it has to spare.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Candidate<'a> {
    pub(crate) account: &'a Account,
    /// The chats of the hub at work on it now.
    pub(crate) working: usize,
}

/// Of `accounts`, each with the chats at work on it, the one Automatic puts a chat of `agent`'s kind on at `now_ms`,
/// or `None` when none has usage to spare, as [`spare`] says. Two with the same to spare go to the one with fewer
/// chats at work on it, then to the first by ID, so the same records always give the same account.
pub(crate) fn choose<'a>(agent: AgentKind, candidates: &[Candidate<'a>], now_ms: i64) -> Option<&'a AccountId> {
    let scored = candidates.iter().filter_map(|candidate| Some((spare(agent, candidate, now_ms)?, candidate)));
    let best = scored.max_by(|(a, x), (b, y)| {
        a.total_cmp(b).then_with(|| y.working.cmp(&x.working)).then_with(|| y.account.id.cmp(&x.account.id))
    });
    best.map(|(_, candidate)| &candidate.account.id)
}

/// How much of its week `candidate`'s account has to spare at `now_ms` for each hour until the week starts again,
/// shared among the chats at work on it and the one to put on it: what's left beyond what it reserves, in percent of the
/// week. `None` for an account a chat of `agent`'s kind can't be put on, since its login isn't one added for the kind
/// that works now, or with none to spare: it reserves all of its week, it has used all of some limit, or what's left of
/// its week is down to what it reserves. A limit that has started again since an agent heard it is all there, and a
/// week no agent has heard is taken to be all there, with all of it to go.
fn spare(agent: AgentKind, candidate: &Candidate<'_>, now_ms: i64) -> Option<f64> {
    let account = candidate.account;
    if !account.works_for(agent, now_ms) || account.used_up(now_ms).next().is_some() {
        return None;
    }
    let week = account.week(now_ms);
    let left = week.map_or(100, protocol::model::UsageLimit::left_percent);
    // All of it reserved is reached at any amount left.
    if account.reserve.is_some_and(|reserve| reserve.reached_at(left)) {
        return None;
    }
    let reserved = account.reserve.map_or(0, Reserve::percent);
    let hours = week.and_then(|week| week.resets_at_ms).map_or(WEEK_HOURS, |at_ms| hours_until(at_ms, now_ms));
    Some(f64::from(left - reserved) / hours.max(FEWEST_HOURS) / to_share(candidate.working))
}

/// How many hours from `now_ms` until `at_ms`.
#[expect(clippy::cast_precision_loss, reason = "a week in milliseconds is far within an f64's exact integers")]
fn hours_until(at_ms: i64, now_ms: i64) -> f64 {
    (at_ms - now_ms) as f64 / HOUR_MS
}

/// How many chats share an account with `working` at work on it already: those and the one to put on it.
#[expect(clippy::cast_precision_loss, reason = "the chats of a hub are far fewer than an f64 counts exactly")]
fn to_share(working: usize) -> f64 {
    (working + 1) as f64
}
