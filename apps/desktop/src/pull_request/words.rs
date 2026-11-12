//! What the bar says for each standing, its colour, and the one thing it offers to do about it.

use look::Work;
use protocol::model::{Checks, QueueState, Standing};

/// The bar's main action.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// Asks the agent, which knows why the changes were made and how this repository works.
    Ask(Ask),
    Archive,
}

/// Something the agent is asked to do with the branch or its pull request.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ask {
    CreatePr,
    CommitAndPush,
    Push,
    Pull,
    /// Brings together a branch and a pull request that each have commits the other doesn't.
    Reconcile,
    ResolveConflicts,
    AddressReview,
    MarkReady,
    FixChecks,
    Merge,
}

/// The colour it's shown in, on the bar and the workspace's mark.
pub(crate) fn work(standing: Standing) -> Work {
    match standing {
        Standing::NoPullRequest => Work::NoPr,
        Standing::Merged => Work::Merged,
        Standing::Closed => Work::Closed,
        Standing::Queued(_) => Work::Queued,
        Standing::Draft => Work::Draft,
        Standing::Ready => Work::Ready,
        // What can't be told is neither good nor bad news, as far as anyone here knows.
        Standing::Unknown
        | Standing::Uncommitted(_)
        | Standing::Diverged
        | Standing::Behind(_)
        | Standing::Unpushed(_)
        | Standing::Conflicts
        | Standing::ChangesRequested
        | Standing::ChecksFailed(_)
        | Standing::ChecksRunning(_)
        | Standing::ReviewRequired
        | Standing::Blocked
        | Standing::Checking => Work::Open,
    }
}

/// What the bar says. `base` is the branch it merges into, if the hub says.
pub(crate) fn title(standing: Standing, base: Option<&str>) -> String {
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

/// What a chat's line for a wait on the pull request says of where it stands on GitHub, which is what the wait goes by,
/// as the bar says it. Nothing once it's merged or closed, which ends the wait, nor where the bar would only say to see
/// it on GitHub. `standing` is GitHub's alone, not the worktree's, and `checks` are the pull request's.
pub(crate) fn for_a_wait(standing: Standing, checks: Checks, base: Option<&str>) -> Option<String> {
    match standing {
        // Only a wait until the checks goes on through a draft, so it says how they stand, while that says anything.
        Standing::Draft if checks.failed > 0 => Some(failed(checks)),
        Standing::Draft if checks.pending > 0 => Some(title(Standing::ChecksRunning(checks), base)),
        // The last four are only the worktree's, which aren't GitHub's.
        Standing::NoPullRequest
        | Standing::Merged
        | Standing::Closed
        | Standing::Unknown
        | Standing::Uncommitted(_)
        | Standing::Diverged
        | Standing::Behind(_)
        | Standing::Unpushed(_) => None,
        Standing::Queued(_)
        | Standing::Conflicts
        | Standing::ChangesRequested
        | Standing::Draft
        | Standing::ChecksFailed(_)
        | Standing::ChecksRunning(_)
        | Standing::ReviewRequired
        | Standing::Blocked
        | Standing::Checking
        | Standing::Ready => Some(title(standing, base)),
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

/// What there is to do, if anything is ours to do. What waits on GitHub or on someone else offers nothing, and so
/// does what can't be told.
pub(crate) fn action(standing: Standing) -> Option<Action> {
    let ask = match standing {
        Standing::Merged | Standing::Closed => return Some(Action::Archive),
        Standing::Queued(_)
        | Standing::ChecksRunning(_)
        | Standing::ReviewRequired
        | Standing::Blocked
        | Standing::Checking
        | Standing::Unknown => return None,
        Standing::NoPullRequest => Ask::CreatePr,
        Standing::Uncommitted(_) => Ask::CommitAndPush,
        Standing::Diverged => Ask::Reconcile,
        Standing::Behind(_) => Ask::Pull,
        Standing::Unpushed(_) => Ask::Push,
        Standing::Conflicts => Ask::ResolveConflicts,
        Standing::ChangesRequested => Ask::AddressReview,
        Standing::Draft => Ask::MarkReady,
        Standing::ChecksFailed(_) => Ask::FixChecks,
        Standing::Ready => Ask::Merge,
    };
    Some(Action::Ask(ask))
}

impl Action {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Action::Archive => "Archive",
            Action::Ask(ask) => match ask {
                Ask::CreatePr => "Create PR",
                // It pushes too, which a pull request's commits always need, and the longer label leaves the bar no
                // room to say what's uncommitted.
                Ask::CommitAndPush => "Commit",
                Ask::Push => "Push",
                Ask::Pull => "Pull",
                Ask::Reconcile => "Sync",
                Ask::ResolveConflicts => "Resolve",
                Ask::AddressReview => "Address",
                Ask::MarkReady => "Ready for review",
                Ask::FixChecks => "Fix checks",
                Ask::Merge => "Merge",
            },
        }
    }
}

impl Ask {
    /// What the agent is asked. `base` is the branch the pull request merges into, or the workspace started from.
    pub(crate) fn prompt(self, base: &str) -> String {
        match self {
            Ask::CreatePr => format!(
                "Create a pull request for this branch. Commit any uncommitted work that belongs in it, push the \
                 branch, and open the PR against `{base}` with `gh pr create`. Write the title and description from \
                 the diff and what we did here. Reply with the PR's URL."
            ),
            Ask::CommitAndPush => "Commit the uncommitted work that belongs in this pull request, and push it. Say \
                                   what you left out, if anything."
                .to_string(),
            Ask::Push => "Push this branch's new commits to its pull request.".to_string(),
            Ask::Pull => {
                "Pull the commits on this pull request that aren't here, and say what they change.".to_string()
            }
            Ask::Reconcile => "This branch and its pull request on GitHub each have commits the other doesn't. Find \
                               out why, bring them together without losing either side's work, and push. Ask me \
                               before you force-push."
                .to_string(),
            Ask::ResolveConflicts => format!(
                "This pull request conflicts with `{base}`. Bring `{base}` in the way this repository does, resolve \
                 the conflicts, check it still builds and passes its tests, and push."
            ),
            Ask::AddressReview => "A reviewer asked for changes to this pull request. Read the review and its \
                                   comments with `gh`, make the changes, push, and say what you changed."
                .to_string(),
            Ask::MarkReady => "Mark this pull request ready for review, with `gh pr ready`.".to_string(),
            Ask::FixChecks => "Checks failed on this pull request. Find out why with `gh pr checks` and the failing \
                               runs' logs, fix what's ours to fix, push, and say what failed and what you did."
                .to_string(),
            Ask::Merge => "Merge this pull request with `gh pr merge`, the way this repository merges, adding it to \
                           the merge queue if it has one. Say what happened."
                .to_string(),
        }
    }
}
