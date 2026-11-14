use super::super::Asked;
use protocol::model::{PrState, PullRequestFound};
use protocol::worker::RateLimit;
use std::time::{Duration, Instant};

/// The calls a minute that reads nobody is waiting on spend once a burst is used up: one a second, so after a restart
/// even two hundred workspaces are all read again within about five minutes. Spent flat out, that's 3,600 of GitHub's
/// 5,000 an hour, so those reads also stop while only [`KEPT_FOR_OTHERS_PERCENT`] of it is left, whoever spent the rest.
pub(super) const PER_MINUTE: u32 = 60;

/// What reads nobody is waiting on can spend at once, such as the first after an app connects.
pub(super) const BURST: u32 = 30;

/// How long what GitHub said of its limit is taken as true before it's asked again. Everything else using it spends
/// it meanwhile, so the hub's count of its own calls only says so much.
pub(super) const GITHUB_SAID_FOR: Duration = Duration::from_mins(1);

/// The part of GitHub's limit that reads nobody is waiting on leave alone, for agents and the user.
const KEPT_FOR_OTHERS_PERCENT: u32 = 20;

/// The calls reading a pull request takes: one to view it, and one more while it's open, for its place in the merge
/// queue and what it has recorded of the queue and of reviews. Going by what was found last time, since what it is now
/// is what's being read, and taking one never read to need both. GitHub's limit counts points rather than calls, and
/// this takes each to cost one, as the worker's do. Not the views the worker makes when the one `gh` finds by the
/// branch's name isn't the one the branch is linked to, for its commits and for the linked one, or when it finds none,
/// for the linked one, which GitHub's own count takes in.
pub(in super::super) fn calls_to_read(found: Option<&PullRequestFound>) -> u32 {
    match found.map(|found| found.pull_request.as_ref()) {
        None => 2,
        Some(Some(pr)) if pr.state == PrState::Open => 2,
        Some(_) => 1,
    }
}

pub(in super::super) struct Budget {
    /// What reads nobody is waiting on may spend now, which grows by [`PER_MINUTE`] up to [`BURST`]. Below zero while
    /// reads someone was waiting on have spent ahead.
    pub(super) calls: f64,
    pub(super) refilled_at: Instant,
    pub(super) github: Option<Said>,
    /// When GitHub was last asked, whether or not it answered.
    pub(super) asked_github_at: Option<Instant>,
}

/// What GitHub last said of its limit.
pub(super) struct Said {
    pub(super) limit: u32,
    pub(super) remaining: u32,
    pub(super) resets_at: Instant,
    /// The calls the hub has made since, which GitHub hadn't counted then.
    pub(super) spent_since: u32,
}

impl Budget {
    pub(in super::super) fn new(now: Instant) -> Self {
        Self { calls: f64::from(BURST), refilled_at: now, github: None, asked_github_at: None }
    }

    /// Whether a read can spend `calls` now, or else when it's worth asking again. One someone is waiting on goes at
    /// once, ahead of the steady rate, unless GitHub has nothing left to give it. One nobody is waiting on waits for
    /// the steady rate, and while GitHub's limit is down to the part kept for everything else, for it to reset. One an
    /// agent asks for goes at once too, but not into the part kept for everything else.
    pub(in super::super) fn allows(&self, calls: u32, asked: Asked, now: Instant) -> Result<(), Instant> {
        if let Some(said) = self.github.as_ref().filter(|said| now < said.resets_at) {
            let kept = match asked {
                Asked::Waiting => 0,
                Asked::Agent | Asked::InTurn => said.limit.saturating_mul(KEPT_FOR_OTHERS_PERCENT) / 100,
            };
            if said.remaining.saturating_sub(said.spent_since) < calls.saturating_add(kept) {
                return Err(said.resets_at);
            }
        }
        let short = f64::from(calls) - self.calls_at(now);
        if asked == Asked::InTurn && short > 0.0 {
            return Err(now + Duration::from_secs_f64(short * 60.0 / f64::from(PER_MINUTE)));
        }
        Ok(())
    }

    /// The same, spending them when it can. Never more than a burst behind, so a run of reads someone waited on
    /// doesn't hold the rest up for long.
    pub(in super::super) fn take(&mut self, calls: u32, asked: Asked, now: Instant) -> Result<(), Instant> {
        self.allows(calls, asked, now)?;
        self.spent(calls, now);
        self.calls = self.calls.max(-f64::from(BURST));
        Ok(())
    }

    /// Pages already read are charged even when the read failed. Future reads wait for their points to refill.
    pub(in crate::hub::pull_requests) fn spent(&mut self, calls: u32, now: Instant) {
        self.calls = self.calls_at(now) - f64::from(calls);
        self.refilled_at = now;
        if let Some(said) = &mut self.github {
            said.spent_since = said.spent_since.saturating_add(calls);
        }
    }

    /// Whether to ask GitHub how much of its limit is left before the next read: once what it said is a minute old,
    /// or has reset, and a minute after asking got no answer, rather than before every read.
    pub(in super::super) fn needs_github(&self, now: Instant) -> bool {
        let stale = self.asked_github_at.is_none_or(|at| now.saturating_duration_since(at) >= GITHUB_SAID_FOR);
        stale || self.github.as_ref().is_some_and(|said| now >= said.resets_at)
    }

    /// `now_ms` is the same moment as `now`, by the wall clock GitHub's reset is given in. A reset too far off to
    /// count to says nothing.
    pub(in super::super) fn github_said(&mut self, limit: RateLimit, now: Instant, now_ms: i64) {
        let resets_in = Duration::from_millis(u64::try_from(limit.resets_at_ms.saturating_sub(now_ms)).unwrap_or(0));
        self.github = now.checked_add(resets_in).map(|resets_at| Said {
            limit: limit.limit,
            remaining: limit.remaining,
            resets_at,
            spent_since: 0,
        });
        self.asked_github_at = Some(now);
    }

    /// Asking got no answer, such as while GitHub can't be reached, so reads go by the steady rate alone for now.
    pub(in super::super) fn github_unknown(&mut self, now: Instant) {
        self.asked_github_at = Some(now);
    }

    pub(super) fn calls_at(&self, now: Instant) -> f64 {
        let elapsed = now.saturating_duration_since(self.refilled_at).as_secs_f64();
        (self.calls + elapsed * f64::from(PER_MINUTE) / 60.0).min(f64::from(BURST))
    }
}
