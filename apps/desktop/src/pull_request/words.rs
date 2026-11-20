//! What the bar says for each standing, and the one thing it offers to do about it. Its colour is
//! [`transcript::work`]'s, which a workspace's mark shows in every app.

use protocol::model::Standing;

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
