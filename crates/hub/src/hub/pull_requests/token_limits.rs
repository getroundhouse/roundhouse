//! What's left of GitHub's hourly limit for each token the hub reads pull requests as, as GitHub said in the answer
//! to the last query read as it. Every GraphQL answer says what's left, so the hub keeps no count of its own: how
//! often each workspace is read sets the pace, and GitHub's word keeps it from spending the part of the limit kept
//! for agents and the person, whose own `gh` shares it.

use std::collections::HashMap;

use parking_lot::Mutex;

use super::Asked;
use super::remote::ReadsAs;
use github::RateLimit;

#[derive(Default)]
pub(in crate::hub::pull_requests) struct TokenLimits(Mutex<HashMap<ReadsAs, RateLimit>>);

impl TokenLimits {
    /// Records `limit`, what GitHub last said is left for `reads_as`, in place of what it said before. An answer that
    /// said nothing changes nothing.
    pub(in crate::hub::pull_requests) fn said(&self, reads_as: &ReadsAs, limit: Option<RateLimit>) {
        if let Some(limit) = limit {
            self.0.lock().insert(reads_as.clone(), limit);
        }
    }

    /// Whether a read as `reads_as` for `asked` can go at `now_ms`, or else when GitHub's limit resets. One someone is
    /// waiting on goes unless nothing is left. One an agent asks for, or nobody does, leaves the part kept for others.
    /// One GitHub hasn't said of since it last reset goes, and finds out.
    pub(in crate::hub::pull_requests) fn allows(
        &self,
        reads_as: &ReadsAs,
        asked: Asked,
        now_ms: i64,
    ) -> Result<(), i64> {
        let limits = self.0.lock();
        let Some(limit) = limits.get(reads_as).filter(|limit| now_ms < limit.resets_at_ms) else { return Ok(()) };
        if limit.remaining > asked.leaving().kept(limit) { Ok(()) } else { Err(limit.resets_at_ms) }
    }
}

#[cfg(test)]
mod tests {
    use protocol::model::GithubAccountId;

    use super::*;

    fn account(n: u64) -> ReadsAs {
        ReadsAs::Account(GithubAccountId::from(format!("account-{n}")))
    }

    /// Each token by what GitHub said of it alone, and none once its limit has reset.
    #[test]
    fn a_read_leaves_the_part_kept_for_others_of_its_own_tokens_limit_until_it_resets() {
        let limits = TokenLimits::default();
        let nearly_spent = RateLimit { limit: 5000, remaining: 900, resets_at_ms: 1_000 };
        limits.said(&account(1), Some(nearly_spent));
        limits.said(&account(2), Some(RateLimit { remaining: 4000, ..nearly_spent }));

        assert_eq!(limits.allows(&account(1), Asked::InTurn, 500), Err(1_000));
        assert_eq!(limits.allows(&account(1), Asked::Agent, 500), Err(1_000));
        assert_eq!(limits.allows(&account(1), Asked::Waiting, 500), Ok(()), "someone is waiting on it");
        assert_eq!(limits.allows(&account(2), Asked::InTurn, 500), Ok(()), "another token's limit is its own");
        assert_eq!(limits.allows(&account(1), Asked::InTurn, 1_000), Ok(()), "once it's reset");
        assert_eq!(limits.allows(&account(3), Asked::InTurn, 500), Ok(()), "GitHub hasn't said");
    }

    #[test]
    fn a_read_someone_waits_on_goes_until_nothing_is_left() {
        let limits = TokenLimits::default();
        limits.said(&account(1), Some(RateLimit { limit: 5000, remaining: 0, resets_at_ms: 1_000 }));

        assert_eq!(limits.allows(&account(1), Asked::Waiting, 500), Err(1_000));
    }
}
