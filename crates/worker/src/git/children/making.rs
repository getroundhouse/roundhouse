use super::super::removal::remove_part_made;
use super::super::{
    branches, commit_of, exists, made_there, make_on_branch, make_on_new_branch, ref_exists, valid_branch_name,
};
use process::Environment;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{CHILDREN, CommitId};
use protocol::worker::ChildBranch;
use std::path::{Component, Path};

/// Makes the worktree at `path`, another of the workspace whose own is at `own`, and says which commit it has
/// checked out. One there already on the branch asked for is what was asked for, made by this ask heard before, or by
/// hand. Whoever calls has `path` to itself.
pub(crate) async fn add_child_worktree(
    given: &Environment,
    own: &Path,
    path: &Path,
    on: &ChildBranch,
    warnings: &mut Vec<String>,
) -> Result<CommitId, ApiError> {
    in_its_folder(own, path)?;
    let branch = match on {
        ChildBranch::New { branch, .. } | ChildBranch::Existing { branch } => branch,
    };
    // Run in the workspace's own worktree, as everything here is, so `HEAD` is the commit it has checked out.
    valid_branch_name(given, own, branch).await?;
    // One a `git worktree add` was stopped part way through, as when the worker went, is nothing git will finish, so
    // it's removed, and made again below.
    let had = branches::checked_out(given, path).await;
    let stopped_part_way = remove_part_made(given, own, path).await?;
    // Its folder being a worktree itself, as one made there by hand would be, would have this one made inside it, and
    // go with it when that one is removed.
    let inside_another = match path.parent() {
        Some(folder) => exists(&folder.join(".git")).await,
        None => false,
    };
    if inside_another || (!stopped_part_way && exists(path).await && !made_there(given, own, path, branch).await) {
        return Err(ApiError::new(ErrorCode::NameTaken, format!("{} is taken", path.display())));
    }
    if !exists(path).await {
        // On the branch git had checked out in it before it was stopped, if that's the one asked for and it's still
        // there, rather than refusing that as a new branch that's taken.
        let its_own = ChildBranch::Existing { branch: branch.clone() };
        let reuse_branch = stopped_part_way
            && had.as_ref() == Some(branch)
            && ref_exists(given, own, &format!("refs/heads/{branch}")).await?;
        match if reuse_branch { &its_own } else { on } {
            ChildBranch::New { branch, base } => make_on_new_branch(given, own, branch, base, path, warnings).await?,
            ChildBranch::Existing { branch } => make_on_branch(given, own, branch, path, warnings).await?,
        }
    }
    commit_of(given, path, "HEAD")
        .await?
        .ok_or_else(|| ApiError::new(ErrorCode::Worker, format!("{} has no commit checked out", path.display())))
}

/// Refuses a `path` that isn't a directory of its own in the workspace's folder of the `.children/` beside its own
/// worktree at `own`, the folder named as that worktree's directory is. So a hub can't have a worktree made outside
/// `.children/`, in another workspace's folder, inside another worktree, or where archiving moves worktrees aside.
fn in_its_folder(own: &Path, path: &Path) -> Result<(), ApiError> {
    let folder = own.parent().unwrap_or(own).join(CHILDREN).join(own.file_name().unwrap_or_default());
    let named =
        |part: Component<'_>| matches!(part, Component::Normal(name) if !name.to_string_lossy().starts_with('.'));
    let mut inside = path.strip_prefix(&folder).map(Path::components).into_iter().flatten();
    if !inside.next().is_some_and(named) || inside.next().is_some() {
        let refused = format!("{} isn't where one of this workspace's other worktrees goes", path.display());
        return Err(ApiError::new(ErrorCode::InvalidRequest, refused));
    }
    Ok(())
}
