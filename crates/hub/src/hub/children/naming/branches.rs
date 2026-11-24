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

/// Which branches the workspace's worktrees have had, as its worker last read them, each `None` while it isn't known,
/// as before the worker has said them since the hub's cache was emptied.
pub(in super::super) struct Had<'a> {
    /// The workspace's own worktree's.
    pub(in super::super) own: Option<&'a Branches>,
    /// Each other worktree it has had, those removed too, with its.
    pub(in super::super) others: &'a [(ChildWorktree, Option<Branches>)],
}

/// That which branches are the workspace's can't be told, since what its worktrees have had isn't known.
#[derive(Debug, PartialEq, Eq)]
pub(in super::super) struct NotKnown;

impl Starts {
    /// What `workspace`'s branches start with, by whether its own worktree has had the branch of its name, as `own`
    /// says. [`NotKnown`] where that's needed and isn't known.
    pub(super) fn of(workspace: &Workspace, own: Option<&Branches>) -> Result<Self, NotKnown> {
        let name = &workspace.name;
        let named = BranchName::from(name.as_str());
        let beside = workspace.branch == named || had(own.ok_or(NotKnown)?, &named);
        let (folder, flat) = (format!("{name}/"), format!("{name}--"));
        if beside {
            Ok(Self { new: flat.clone(), any: vec![folder, flat] })
        } else {
            Ok(Self { new: folder.clone(), any: vec![folder] })
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

/// The new branches to try for `name`, and where each one's worktree goes, for the workspace whose own is at `own`, and
/// has had `branches`.
pub(in super::super) fn new_branches(
    workspace: &Workspace,
    branches: Option<&Branches>,
    own: &str,
    name: &WorkName,
) -> Result<Vec<Try>, NotKnown> {
    let start = Starts::of(workspace, branches)?.new;
    Ok(numbered(name.as_str(), |work| Try {
        branch: BranchName::from(format!("{start}{work}")),
        path: child_path(own, &workspace.name, work),
    }))
}

/// Where a worktree of `branch` goes, and the numbered places after it, or `None` if it isn't one of the workspace's:
/// one in its folder, the one its record says, one another of its worktrees was made on, or one any of them has had,
/// as `branches` says. [`NotKnown`] where it isn't the workspace's by what's known, and what a worktree has had isn't.
pub(in super::super) fn places_of_its(
    workspace: &Workspace,
    branches: &Had<'_>,
    own: &str,
    branch: &BranchName,
) -> Result<Option<Vec<Try>>, NotKnown> {
    // Its folder is its whatever its own worktree has had, which says only whether those beside its name are too.
    let folder = || vec![format!("{}/", workspace.name)];
    let starts = Starts::of(workspace, branches.own).map_or_else(|NotKnown| folder(), |starts| starts.any);
    let after = starts.iter().find_map(|start| branch.as_str().strip_prefix(start.as_str()));
    let recorded = workspace.branch == *branch || branches.others.iter().any(|(other, _)| other.branch == *branch);
    let worktrees = std::iter::once(branches.own).chain(branches.others.iter().map(|(_, had)| had.as_ref()));
    if after.is_none() && !recorded && !had_by_any(worktrees, branch)? {
        return Ok(None);
    }
    // One name, so a worktree is never made inside another, as `euston/x/y`'s would be inside `euston/x`'s.
    let work = after.unwrap_or(branch.as_str()).replace('/', "-");
    Ok(Some(numbered(&work, |work| Try { branch: branch.clone(), path: child_path(own, &workspace.name, work) })))
}

/// Whether any of `each`, a worktree's branches each, says it has had `branch` checked out. [`NotKnown`] where none
/// known does, and one isn't known.
fn had_by_any<'a>(each: impl Iterator<Item = Option<&'a Branches>>, branch: &BranchName) -> Result<bool, NotKnown> {
    let mut all_known = true;
    for branches in each {
        let Some(branches) = branches else {
            all_known = false;
            continue;
        };
        if had(branches, branch) {
            return Ok(true);
        }
    }
    if all_known { Ok(false) } else { Err(NotKnown) }
}

/// Whether `branches` say their worktree has had `branch` checked out.
fn had(branches: &Branches, branch: &BranchName) -> bool {
    branches.had.iter().any(|had| had.name == *branch)
}
