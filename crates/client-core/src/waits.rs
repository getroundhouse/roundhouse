//! What a client says of a wait on pull requests: which it watches, until what, and why it ended, so the app and the
//! command line say the same.

use protocol::model::{WaitEnd, WaitUntil, Watching};

/// Which pull requests a wait watches, and until what, as "Waiting on" and "Waited on" go on to say: "#7, #8: required
/// checks", or for one on the branch's pull request, "#7's checks" or "#7".
pub fn watched(watching: &Watching) -> String {
    match watching {
        Watching::PullRequests { numbers, until } => {
            let numbers: Vec<String> = numbers.iter().map(|number| format!("#{number}")).collect();
            let what = match until {
                WaitUntil::RequiredChecks => "required checks",
                WaitUntil::AllChecks | WaitUntil::Checks => "checks",
                WaitUntil::Merged | WaitUntil::Done | WaitUntil::Unknown => "merge",
            };
            format!("{}: {what}", numbers.join(", "))
        }
        Watching::Branch { number, until } => match until {
            WaitUntil::Checks | WaitUntil::RequiredChecks | WaitUntil::AllChecks => format!("#{number}'s checks"),
            WaitUntil::Done | WaitUntil::Merged | WaitUntil::Unknown => format!("#{number}"),
        },
        Watching::Unknown => "a pull request".to_string(),
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

    use super::watched;

    /// A wait asked about pull requests names them, and what for. One on the branch's names it as the branch's.
    #[test]
    fn a_wait_says_which_pull_requests_it_watches_and_until_what() {
        let asked = |until| Watching::PullRequests { numbers: vec![6, 7], until };
        let branch = |until| Watching::Branch { number: 7, until };

        assert_eq!(watched(&asked(WaitUntil::RequiredChecks)), "#6, #7: required checks");
        assert_eq!(watched(&asked(WaitUntil::Merged)), "#6, #7: merge");
        assert_eq!(watched(&branch(WaitUntil::Checks)), "#7's checks");
        assert_eq!(watched(&branch(WaitUntil::Done)), "#7");
        assert_eq!(watched(&Watching::Unknown), "a pull request", "one a newer hub knows");
    }
}
