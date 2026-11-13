use protocol::model::{BranchName, Branches, CHILDREN, ChildWorktree, WorkName, Workspace};

/// How many numbers a taken name tries before giving up, which only a repository with that many worktrees of one name
/// reaches.
const TRIES: usize = 50;

/// A worktree to try making: its branch, and where it goes.
#[derive(Debug, PartialEq, Eq)]
pub(in super::super) struct Try {
    pub(in super::super) branch: BranchName,
    pub(in super::super) path: String,
}

/// What a workspace's branches start with.
struct Starts {
    /// What a new branch starts with: the workspace's folder, `euston/`. But one made before its branches went in a
    /// folder, which still has its branch `euston`, names them beside it, as `euston--try-the-parser`, since git can't
    /// have `euston/…` beside `euston`.
    pub(super) new: String,
    /// What any of its branches may start with, which for that older workspace is either.
    pub(super) any: Vec<String>,
}

impl Starts {
    pub(super) fn of(workspace: &Workspace) -> Self {
        let name = &workspace.name;
        let own = BranchName::from(name.as_str());
        let beside = workspace.branch == own || had(workspace.branches.as_ref(), &own);
        let (folder, flat) = (format!("{name}/"), format!("{name}--"));
        if beside {
            Self { new: flat.clone(), any: vec![folder, flat] }
        } else {
            Self { new: folder.clone(), any: vec![folder] }
        }
    }
}

/// Where the worktree named `work` goes for the workspace `folder`, whose own worktree is at `own`: in a folder of
/// the workspace's name in [`CHILDREN`], beside the workspaces. Spelled with the last separator in `own`, so it reads
/// as a path on the worker's system, whatever system the hub runs on.
pub(in super::super) fn child_path(own: &str, folder: &str, work: &str) -> String {
    let own = own.trim_end_matches(['/', '\\']);
    let (beside, separator) = match own.rfind(['/', '\\']) {
        Some(at) => (&own[..=at], &own[at..=at]),
        None => ("", "/"),
    };
    format!("{beside}{CHILDREN}{separator}{folder}{separator}{work}")
}

/// `work`, then `work-2`, `work-3` and so on, each with what `place` makes of it.
fn numbered(work: &str, place: impl Fn(&str) -> Try) -> Vec<Try> {
    (1..=TRIES).map(|n| if n == 1 { place(work) } else { place(&format!("{work}-{n}")) }).collect()
}

/// The new branches to try for `name`, and where each one's worktree goes, for the workspace whose own is at `own`.
pub(in super::super) fn new_branches(workspace: &Workspace, own: &str, name: &WorkName) -> Vec<Try> {
    let start = Starts::of(workspace).new;
    numbered(name.as_str(), |work| Try {
        branch: BranchName::from(format!("{start}{work}")),
        path: child_path(own, &workspace.name, work),
    })
}

/// Where a worktree of `branch` goes, and the numbered places after it, or `None` if it isn't one of the workspace's:
/// one in its folder, one its own worktree has had, or one another of its worktrees, in `others`, was made on or has
/// had.
pub(in super::super) fn places_of_its(
    workspace: &Workspace,
    own: &str,
    branch: &BranchName,
    others: &[ChildWorktree],
) -> Option<Vec<Try>> {
    let starts = Starts::of(workspace).any;
    let after = starts.iter().find_map(|start| branch.as_str().strip_prefix(start.as_str()));
    let theirs = after.is_some()
        || workspace.branch == *branch
        || had(workspace.branches.as_ref(), branch)
        || others.iter().any(|other| other.branch == *branch || had(other.branches.as_ref(), branch));
    if !theirs {
        return None;
    }
    // One name, so a worktree is never made inside another, as `euston/x/y`'s would be inside `euston/x`'s.
    let work = after.unwrap_or(branch.as_str()).replace('/', "-");
    Some(numbered(&work, |work| Try { branch: branch.clone(), path: child_path(own, &workspace.name, work) }))
}

/// Whether `branches` say their worktree has had `branch` checked out.
fn had(branches: Option<&Branches>, branch: &BranchName) -> bool {
    branches.is_some_and(|branches| branches.had.iter().any(|had| had.name == *branch))
}
