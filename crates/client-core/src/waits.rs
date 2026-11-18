//! What a client says of a wait on pull requests: which it watches, until what, and why it ended, so the app and the
//! command line say the same.

use protocol::model::{
    BranchName, CheckState, ObservedPullRequest, PrState, PullRequestsSnapshot, Standing, Wait, WaitEnd, WaitUntil,
    Watching,
};

use crate::standing;

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

/// What a wait's line says while it's open, from the hub's last reading of its pull requests. A wait until checks says
/// what it's for once, in the count, as "Waiting on #6, #7 · 2 required checks running", since only those count, and
/// the bar counts every check. A wait until a merge says where the pull request holding it stands, in the bar's words,
/// as "Waiting on #9 to merge · Queued to merge", naming it when the wait watches several, as "Waiting on #6, #7 to
/// merge · #6: Review required". A wait the singular tool started, which only an older hub can still have open, has no
/// reading, so it says only what it waits on.
pub fn waiting(wait: &Wait) -> String {
    let watching = wait.watching();
    let what = watched(&watching);
    let Some(watch) = &wait.watch else { return format!("Waiting on {what}") };
    if watch.read_error.is_some() {
        return format!("Waiting on {what} \u{b7} couldn't refresh GitHub");
    }
    let numbers = numbers_of(&watching_numbers(&watching));
    let snapshot = &watch.snapshot;
    match wait.until {
        WaitUntil::RequiredChecks => {
            format!("Waiting on {numbers} \u{b7} {}", running(snapshot, true, "required check"))
        }
        WaitUntil::AllChecks | WaitUntil::Checks => {
            format!("Waiting on {numbers} \u{b7} {}", running(snapshot, false, "check"))
        }
        WaitUntil::Merged | WaitUntil::Done | WaitUntil::Unknown => match holding(snapshot) {
            Some(stands) => format!("Waiting on {what} \u{b7} {stands}"),
            None => format!("Waiting on {what}"),
        },
    }
}

fn watching_numbers(watching: &Watching) -> Vec<u64> {
    match watching {
        Watching::PullRequests { numbers, .. } => numbers.clone(),
        Watching::Branch { number, .. } => vec![*number],
        Watching::Unknown => Vec::new(),
    }
}

/// How many of the checks a wait goes by are still running, across its pull requests, as "2 required checks running".
fn running(snapshot: &PullRequestsSnapshot, required_only: bool, check: &str) -> String {
    let running = snapshot
        .pull_requests
        .iter()
        .flat_map(|row| &row.checks)
        .filter(|check| {
            (!required_only || check.required) && matches!(check.state, CheckState::Pending | CheckState::Unknown)
        })
        .count();
    if running == 1 { format!("1 {check} running") } else { format!("{running} {check}s running") }
}

/// Where the pull request holding a wait until a merge stands, in the bar's words: the lowest of its stack that hasn't
/// merged, named when there are several. Nothing where the bar would only say to see it on GitHub.
fn holding(snapshot: &PullRequestsSnapshot) -> Option<String> {
    let open: Vec<_> = snapshot.pull_requests.iter().filter(|row| row.pull_request.state != PrState::Merged).collect();
    let waits_on_another = |row: &ObservedPullRequest| {
        row.dependencies.iter().any(|below| open.iter().any(|other| other.pull_request.number == below.get()))
    };
    let lowest = open.iter().find(|row| !waits_on_another(row)).or(open.first())?;
    let pr = &lowest.pull_request;
    let stands = match Standing::on_github_alone(pr) {
        Standing::NoPullRequest | Standing::Merged | Standing::Closed | Standing::Unknown => return None,
        standing @ (Standing::Queued(_)
        | Standing::Uncommitted(_)
        | Standing::Diverged
        | Standing::Behind(_)
        | Standing::Unpushed(_)
        | Standing::Conflicts
        | Standing::ChangesRequested
        | Standing::Draft
        | Standing::ChecksFailed(_)
        | Standing::ChecksRunning(_)
        | Standing::ReviewRequired
        | Standing::Blocked
        | Standing::Checking
        | Standing::Ready) => standing::title(standing, pr.base.as_ref().map(BranchName::as_str)),
    };
    Some(if snapshot.pull_requests.len() > 1 { format!("#{}: {stands}", pr.number) } else { stands })
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
    use protocol::fixtures::{observed_pull_request, pull_request_wait};
    use protocol::model::{
        Checks, PrState, PullRequest, PullRequestNumber, QueueState, Review, Wait, WaitUntil, Watching,
    };

    use super::{waiting, watched};

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

    /// A wait until checks counts the checks it goes by, naming required ones, which the bar doesn't count alone. A
    /// wait until a merge says where its pull request stands in the bar's words, rather than a count, which is 0 in the
    /// merge queue, whose checks run on the queue's commit.
    #[test]
    fn a_waits_line_says_its_count_or_where_its_pull_request_stands() {
        let wait = pull_request_wait("w1");
        let with = |until, change: fn(&mut PullRequest)| {
            let mut wait = Wait { until, ..wait.clone() };
            change(&mut wait.watch.as_mut().unwrap().snapshot.pull_requests[0].pull_request);
            waiting(&wait)
        };

        assert_eq!(with(WaitUntil::RequiredChecks, |_| {}), "Waiting on #7 \u{b7} 1 required check running");
        assert_eq!(with(WaitUntil::AllChecks, |_| {}), "Waiting on #7 \u{b7} 1 check running");
        assert_eq!(with(WaitUntil::Merged, |_| {}), "Waiting on #7 to merge \u{b7} 1 check running");
        let queued = |pr: &mut PullRequest| {
            pr.checks = Checks { passed: 1, failed: 0, pending: 0 };
            pr.queue = Some(QueueState::AwaitingChecks);
        };
        assert_eq!(with(WaitUntil::Merged, queued), "Waiting on #7 to merge \u{b7} Running checks in the queue");
        let ready = |pr: &mut PullRequest| pr.checks = Checks { passed: 1, failed: 0, pending: 0 };
        assert_eq!(with(WaitUntil::Merged, ready), "Waiting on #7 to merge \u{b7} Ready to merge");
        let unknown = |pr: &mut PullRequest| pr.state = PrState::Unknown;
        assert_eq!(with(WaitUntil::Merged, unknown), "Waiting on #7 to merge", "nothing but to see it on GitHub");
    }

    /// A wait on a stack is held by the lowest of it that hasn't merged, which its line names.
    #[test]
    fn a_wait_until_a_stack_merges_names_the_pull_request_holding_it() {
        let mut wait = Wait { until: WaitUntil::Merged, ..pull_request_wait("w1") };
        let snapshot = &mut wait.watch.as_mut().unwrap().snapshot;
        let mut lower = observed_pull_request(6);
        lower.pull_request.checks = Checks { passed: 1, failed: 0, pending: 0 };
        lower.pull_request.review = Review::Required;
        snapshot.pull_requests[0].dependencies = vec![PullRequestNumber::try_from(6).unwrap()];
        snapshot.pull_requests.push(lower);

        assert_eq!(waiting(&wait), "Waiting on #7, #6 to merge \u{b7} #6: Review required");
        let snapshot = &mut wait.watch.as_mut().unwrap().snapshot;
        snapshot.pull_requests[1].pull_request.state = PrState::Merged;
        assert_eq!(waiting(&wait), "Waiting on #7, #6 to merge \u{b7} #7: 1 check running", "once #6 has merged");
    }

    /// A wait the singular tool started has no reading of its pull request, so its line says only what it waits on.
    #[test]
    fn a_wait_with_no_reading_says_only_what_it_waits_on() {
        let wait = Wait { watch: None, until: WaitUntil::Checks, ..pull_request_wait("w1") };

        assert_eq!(waiting(&wait), "Waiting on #7's checks");
        assert_eq!(waiting(&Wait { until: WaitUntil::Done, ..wait }), "Waiting on #7");
    }

    /// GitHub couldn't be read, so there's no count, and the line says what it waits until after the numbers.
    #[test]
    fn a_wait_that_couldnt_read_github_says_so() {
        let mut wait = pull_request_wait("w1");
        wait.watch.as_mut().unwrap().read_error = Some("offline".into());

        assert_eq!(waiting(&wait), "Waiting on #7 for required checks \u{b7} couldn't refresh GitHub");
    }
}
