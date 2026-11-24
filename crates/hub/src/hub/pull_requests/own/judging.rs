//! What reading a workspace's own pull request asks of GitHub, as its record says, which of what GitHub finds is the
//! workspace's, and how its worktree stands against it, each from what's known with nothing to run.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{Branch, BranchName, Branches, Head, Local, PrState, RepoUrl, Upstream, Workspace};

use github::{Headed, HeadsAsk};

/// What reading a workspace's pull request asks of GitHub.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::hub) enum Wanted {
    /// Nothing, since there's none to find: HEAD is detached, so there's no branch, which `on` says, or the branch
    /// checked out is pushed nowhere yet, or only to another branch of the repository itself.
    Nothing { on: Option<BranchName> },
    /// Nothing that can be told, since the workspace's branches aren't known, as before its worker has said them since
    /// the hub's cache was emptied. So nothing is read or recorded, and the pull request the workspace has stays as of
    /// its last read: none known isn't none.
    NotKnown,
    /// The pull requests whose head is the branch on GitHub that `on`, the branch checked out, pushes to.
    Heads { on: BranchName, ask: HeadsAsk },
    /// Nothing that can be read, and why: the repository isn't on github.com, or `on` pushes somewhere that isn't.
    Unreadable { on: BranchName, error: ApiError },
}

/// What reading a workspace's pull request asks of GitHub, by the branch it has checked out and where that's pushed,
/// as `branches`, its worktree's, say, in the repository at `repository`, the workspace's repository's own address. Its pull requests
/// are in that repository, the one its work merges into, and their head is the upstream's branch, of the repository
/// the upstream's remote names, which is a fork's when it's pushed to one. A remote whose address isn't known, as one
/// git has no address for, is taken for the repository's own. An upstream of another name than the branch's
/// is where the branch was taken from, as `git switch -c next origin/main` leaves it, not where it's pushed, since git
/// pushes a branch to the branch of its own name unless it's told otherwise each time, so it's pushed nowhere yet,
/// rather than having the pull requests of `main`. *Unconfirmed.*
pub(in crate::hub) fn wanted(branches: Option<&Branches>, repository: Option<&RepoUrl>) -> Wanted {
    let Some(branches) = branches else { return Wanted::NotKnown };
    let Some(branch) = branches.checked_out() else {
        return Wanted::Nothing { on: None };
    };
    let on = branch.name.clone();
    let pushed = |upstream: &&Upstream| upstream.remote.as_str() != "." && upstream.name == branch.name;
    let Some(upstream) = branch.upstream.as_ref().filter(pushed) else {
        return Wanted::Nothing { on: Some(on) };
    };
    let Some(into) = repository.and_then(RepoUrl::on_github) else {
        let error = ApiError::new(
            ErrorCode::Unavailable,
            "the repository isn't on github.com, so it has no pull request the hub can read",
        );
        return Wanted::Unreadable { on, error };
    };
    let from = match upstream.address.as_ref().map(RepoUrl::on_github) {
        None => into.clone(),
        Some(Some(from)) => from,
        Some(None) => {
            let error = ApiError::new(
                ErrorCode::Unavailable,
                format!("{on} is pushed to {}, which isn't on github.com", upstream.remote.as_str()),
            );
            return Wanted::Unreadable { on, error };
        }
    };
    Wanted::Heads { ask: HeadsAsk { repository: into, from, branch: upstream.name.clone() }, on }
}

/// Which of `headed`, the pull requests of the branch on GitHub `branch` pushes to, newest first, is the workspace's:
/// an open one, or else the one it had, at the address `known`, as one that has merged or closed since, or else the newest whose head
/// is where `branch` or its upstream is. GitHub keeps the pull requests of a branch name that's been used again, once
/// the branch it was is merged and deleted, so a merged or closed one first seen now is the workspace's only if its
/// head is a commit the branch has. `None` when it has none.
pub(in crate::hub) fn chosen<'a>(headed: &'a [Headed], known: Option<&str>, branch: &Branch) -> Option<&'a Headed> {
    if let Some(open) = headed.iter().find(|pr| pr.state == PrState::Open) {
        return Some(open);
    }
    if let Some(known) = known.and_then(|known| headed.iter().find(|pr| pr.url == known)) {
        return Some(known);
    }
    let upstream = branch.upstream.as_ref().and_then(|upstream| upstream.tip.as_ref());
    headed.iter().find(|pr| pr.head == branch.tip || Some(&pr.head) == upstream)
}

/// How the worktree stands against a pull request whose newest commit is `head`, by `branch` and its upstream as the
/// worker last read them, with `uncommitted` files. Against the upstream where the pull request's head is the
/// upstream's or the branch's own, as it is once it's pushed from here or fetched. Otherwise the pull request has at
/// least one commit the worktree hasn't fetched, such as one someone else pushed, or GitHub's Update branch made.
pub(in crate::hub) fn local(head: &protocol::model::CommitId, branch: &Branch, uncommitted: u32) -> Local {
    let upstream = branch.upstream.as_ref();
    let apart = upstream.and_then(|upstream| upstream.apart).unwrap_or_default();
    if *head == branch.tip {
        return Local { uncommitted, ahead: 0, behind: 0 };
    }
    let (ahead, behind) = match upstream.and_then(|upstream| upstream.tip.as_ref()) {
        Some(tip) if tip == head => (apart.ahead, apart.behind),
        _ => (apart.ahead, apart.behind.max(1)),
    };
    Local { uncommitted, ahead, behind }
}

/// Whether what the hub last found of `workspace`'s pull request was for another branch than the one checked out, as
/// `branches`, its worktree's, say, so it's no longer what the workspace's branch has, whatever it says. Not when
/// nothing's been found yet, nor when it's not known what's checked out, or HEAD is detached, which has no pull request.
/// Nor when none was found on no branch, as where GitHub can't be read at all, such as without a login, which checking
/// out another doesn't change.
pub(in crate::hub) fn found_on_another_branch(workspace: &Workspace, branches: Option<&Branches>) -> bool {
    let Some(Head::Branch { name }) = branches.map(|branches| &branches.head) else { return false };
    let found_on_a_branch = workspace.pull_request_link.is_some() && workspace.pull_request_branch.is_some();
    let found = workspace.pull_request_linked().is_some() || found_on_a_branch;
    found && workspace.pull_request_branch.as_ref() != Some(name)
}
