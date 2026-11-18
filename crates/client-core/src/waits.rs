//! What a client says of a wait on pull requests: which it watches, until what, and why it ended, so the app and the
//! command line say the same.

use protocol::model::{WaitEnd, WaitUntil, Watching};

/// Which pull requests a wait watches, and until what, as "Waiting on" and "Waited on" go on to say: "#7, #8 for
/// required checks", "#7 to merge", or for one on the branch's pull request, "#7's checks" or "#7".
pub fn watched(watching: &Watching) -> String {
    match watching {
        Watching::PullRequests { numbers, until } => format!("{} {}", numbers_of(numbers), until_what(*until)),
        Watching::Branch { number, until } => match until {
            WaitUntil::Checks | WaitUntil::RequiredChecks | WaitUntil::AllChecks => format!("#{number}'s checks"),
            WaitUntil::Done | WaitUntil::Merged | WaitUntil::Unknown => format!("#{number}"),
        },
        Watching::Unknown => "a pull request".to_string(),
    }
}

/// What "Waiting on" goes on to say while `running` of the checks a wait goes by are still running: which pull
/// requests, and a count that ends as the bar's does, "N checks running". A wait until checks says what it's for once,
/// in the count, as "#6, #7 · 2 required checks running", since only those count, and the bar counts every check. A
/// wait until a merge says it after the numbers, as "#9 to merge · 1 check running".
pub fn watched_running(watching: &Watching, running: usize) -> String {
    let count = |checks: &str| {
        let checks = if running == 1 { checks.strip_suffix('s').unwrap_or(checks) } else { checks };
        format!("{running} {checks} running")
    };
    match watching {
        Watching::PullRequests { numbers, until: WaitUntil::RequiredChecks } => {
            format!("{} \u{b7} {}", numbers_of(numbers), count("required checks"))
        }
        Watching::PullRequests { numbers, until: WaitUntil::AllChecks | WaitUntil::Checks } => {
            format!("{} \u{b7} {}", numbers_of(numbers), count("checks"))
        }
        Watching::PullRequests { until: WaitUntil::Merged | WaitUntil::Done | WaitUntil::Unknown, .. }
        | Watching::Branch { .. }
        | Watching::Unknown => format!("{} \u{b7} {}", watched(watching), count("checks")),
    }
}

fn numbers_of(numbers: &[u64]) -> String {
    numbers.iter().map(|number| format!("#{number}")).collect::<Vec<_>>().join(", ")
}

fn until_what(until: WaitUntil) -> &'static str {
    match until {
        WaitUntil::RequiredChecks => "for required checks",
        WaitUntil::AllChecks | WaitUntil::Checks => "for checks",
        WaitUntil::Merged | WaitUntil::Done | WaitUntil::Unknown => "to merge",
    }
}

/// Why a wait ended, in a word or two, as the worker tells the agent in a sentence.
pub fn why_it_ended(how: WaitEnd) -> &'static str {
    match how {
        WaitEnd::Merged => "Merged",
        WaitEnd::Closed => "Closed without merging",
        WaitEnd::Conflicts => "Conflicts with its base",
        WaitEnd::ChecksFailed => "A check failed",
        WaitEnd::ChangesRequested => "Changes requested",
        WaitEnd::LeftQueue => "Left the merge queue",
        WaitEnd::Draft => "It's a draft",
        WaitEnd::ChecksPassed => "Checks passed",
        WaitEnd::NoPullRequest => "No pull request",
        WaitEnd::StackChanged => "Its stack changed",
        WaitEnd::NeedsRebase => "Needs a rebase",
        WaitEnd::TimedOut => "Timed out",
        WaitEnd::Unavailable => "GitHub unavailable",
        WaitEnd::Cancelled => "Called off",
        WaitEnd::CalledOffByPerson => "Stopped",
        WaitEnd::Replaced => "Replaced by another wait",
        WaitEnd::Unknown => "Ended",
    }
}

#[cfg(test)]
mod tests {
    use protocol::model::{WaitUntil, Watching};

    use super::{watched, watched_running};

    /// A wait asked about pull requests names them, and what for. One on the branch's names it as the branch's.
    #[test]
    fn a_wait_says_which_pull_requests_it_watches_and_until_what() {
        let asked = |until| Watching::PullRequests { numbers: vec![6, 7], until };
        let branch = |until| Watching::Branch { number: 7, until };

        assert_eq!(watched(&asked(WaitUntil::RequiredChecks)), "#6, #7 for required checks");
        assert_eq!(watched(&asked(WaitUntil::AllChecks)), "#6, #7 for checks");
        assert_eq!(watched(&asked(WaitUntil::Merged)), "#6, #7 to merge");
        assert_eq!(watched(&branch(WaitUntil::Checks)), "#7's checks");
        assert_eq!(watched(&branch(WaitUntil::Done)), "#7");
        assert_eq!(watched(&Watching::Unknown), "a pull request", "one a newer hub knows");
    }

    /// Every count ends as the bar's does, and says what the wait is for only once: a wait until checks in the count,
    /// naming required ones, which the bar doesn't count alone, and a wait until a merge after the numbers.
    #[test]
    fn a_waits_count_names_what_it_counts_once() {
        let asked = |until| Watching::PullRequests { numbers: vec![6, 7], until };

        assert_eq!(watched_running(&asked(WaitUntil::RequiredChecks), 1), "#6, #7 \u{b7} 1 required check running");
        assert_eq!(watched_running(&asked(WaitUntil::RequiredChecks), 2), "#6, #7 \u{b7} 2 required checks running");
        assert_eq!(watched_running(&asked(WaitUntil::AllChecks), 0), "#6, #7 \u{b7} 0 checks running");
        assert_eq!(watched_running(&asked(WaitUntil::Merged), 1), "#6, #7 to merge \u{b7} 1 check running");
    }
}
