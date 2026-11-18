//! Where a pull request stands, as the bar says it, so a chat's line for a wait on one says it in the same words.

use protocol::model::{Checks, QueueState, Standing};

/// What the bar says. `base` is the branch it merges into, if the hub says.
pub fn title(standing: Standing, base: Option<&str>) -> String {
    match standing {
        Standing::NoPullRequest => "No pull request".to_string(),
        Standing::Merged => "Merged".to_string(),
        Standing::Closed => "Closed".to_string(),
        Standing::Queued(queue) => queued(queue).to_string(),
        Standing::Uncommitted(files) => count(files, "uncommitted file"),
        Standing::Diverged => "Diverged from GitHub".to_string(),
        Standing::Behind(commits) => format!("{} to pull", count(commits, "commit")),
        Standing::Unpushed(commits) => format!("{} to push", count(commits, "commit")),
        Standing::Conflicts => format!("Conflicts with {}", base.unwrap_or("its base")),
        Standing::ChangesRequested => "Changes requested".to_string(),
        Standing::Draft => "Draft".to_string(),
        Standing::ChecksFailed(checks) => failed(checks),
        Standing::ChecksRunning(checks) => format!("{} running", count(checks.pending, "check")),
        Standing::ReviewRequired => "Review required".to_string(),
        Standing::Blocked => "Blocked from merging".to_string(),
        Standing::Checking => "Checking whether it can merge".to_string(),
        Standing::Ready => "Ready to merge".to_string(),
        // The number beside it opens it there.
        Standing::Unknown => "See it on GitHub".to_string(),
    }
}

fn queued(queue: QueueState) -> &'static str {
    match queue {
        QueueState::AwaitingChecks => "Running checks in the queue",
        QueueState::Locked => "Merge queue locked",
        QueueState::Unmergeable => "Failed in the merge queue",
        QueueState::Mergeable | QueueState::Queued | QueueState::Unknown => "Queued to merge",
    }
}

/// Only how many failed, which is what there is to do, so it fits beside the action.
fn failed(checks: Checks) -> String {
    format!("{} failed", count(checks.failed, "check"))
}

fn count(n: u32, thing: &str) -> String {
    if n == 1 { format!("1 {thing}") } else { format!("{n} {thing}s") }
}

#[cfg(test)]
mod tests {
    use protocol::model::{Checks, Standing};

    use super::title;

    #[test]
    fn the_bar_says_how_many_and_of_what() {
        let failing = Checks { passed: 6, failed: 2, pending: 1 };

        assert_eq!(title(Standing::ChecksFailed(failing), Some("main")), "2 checks failed");
        assert_eq!(
            title(Standing::ChecksRunning(Checks { pending: 1, ..Checks::default() }), Some("main")),
            "1 check running"
        );
        assert_eq!(title(Standing::Unpushed(3), Some("main")), "3 commits to push");
        assert_eq!(title(Standing::Uncommitted(1), Some("main")), "1 uncommitted file");
        assert_eq!(title(Standing::Conflicts, Some("trunk")), "Conflicts with trunk");
        assert_eq!(title(Standing::Conflicts, None), "Conflicts with its base");
    }
}
