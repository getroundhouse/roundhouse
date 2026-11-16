//! Bringing the copy and its patches onto a release, and continuing once conflicts in a patch are settled.

use std::path::{Path, PathBuf};

use crate::copy::{COPY, ignored, marked_in, replace, replace_stamped, tracked};
use crate::cut::cut;
use crate::digest::tree_of_dir;
use crate::error::Error;
use crate::files::{CopyPath, files_under};
use crate::git::{Git, Settings};
use crate::patches::{PatchName, remove_patch, write_patch};
use crate::pin::{Commit, Kept, Pin};
use crate::record::{Record, Settling, Tree};
use crate::scratch::{CherryPick, apply_exact, apply_for_old, cherry_pick, committed, diff_from, head, init, work_dir};
use crate::sync::Synced;
use crate::upstream::fetch;
use crate::versions::pin_versions;

/// Makes the copy at `pin`'s release when there's no copy yet.
pub(crate) fn first_copy(git: &Git, root: &Path, pin: &Pin) -> Result<Synced, Error> {
    let copy = root.join(COPY);
    let theirs = if copy.exists() { tracked(git, root)? } else { Vec::new() };
    if !theirs.is_empty() {
        return Err(Error::Unrecorded(theirs.into_iter().take(5).collect()));
    }
    let work = work_dir()?;
    let new_base = cut_checked(git, root, work.path(), &pin.upstream, &pin.commit, &pin.kept)?;
    let tree = tree_of_dir(&new_base)?;
    let repo = work.path().join("repo");
    init(git, &repo)?;
    committed(git, &repo, &new_base, &files_under(&new_base)?, "Base")?;
    replace_stamped(&copy, &[], &repo, &new_base)?;
    Record {
        release: pin.release.clone(),
        commit: pin.commit.clone(),
        tree,
        kept: pin.kept.clone(),
        patches: Vec::new(),
        settling: None,
    }
    .write(root)?;
    Ok(Synced::Brought { moved: pin_versions(root)?, dropped: Vec::new() })
}

/// Moves the copy and each patch in `made` onto `pin`'s release.
pub(crate) fn move_release(git: &Git, root: &Path, pin: &Pin, made: &Record) -> Result<Synced, Error> {
    let work = work_dir()?;
    let new_base = cut_checked(git, root, work.path(), &pin.upstream, &pin.commit, &pin.kept)?;
    let repo = work.path().join("repo");
    let old = build_old_commits(
        git,
        root,
        work.path(),
        &repo,
        &OldRelease { upstream: &pin.upstream, commit: &made.commit, kept: &made.kept, patches: &made.patches },
    )?;
    let new_tree = start_new_branch(git, &repo, &new_base)?;
    let step = PickStep {
        pin,
        new_base: &new_base,
        new_tree,
        kept_patches: Vec::new(),
        dropped: Vec::new(),
        from: &made.commit,
        from_kept: &made.kept,
    };
    pick_patches(git, root, &repo, step, &old)
}

/// Saves the settled patch in `settling` and continues cherry-picking any patches after it onto `pin`'s release.
pub(crate) fn resume_settling(
    git: &Git,
    root: &Path,
    pin: &Pin,
    made: &Record,
    settling: Settling,
) -> Result<Synced, Error> {
    let copy = root.join(COPY);
    let ours = tracked(git, root)?;
    check_settled(&copy, &ours, &settling.files)?;
    if (&made.commit, &made.kept) != (&pin.commit, &pin.kept) {
        return Err(Error::PinnedMovedWhileSettling);
    }
    let at = made
        .patches
        .iter()
        .position(|existing| existing == &settling.patch)
        .ok_or_else(|| Error::NoSuchPatch(settling.patch.clone()))?;

    let work = work_dir()?;
    let new_base = cut_checked(git, root, work.path(), &pin.upstream, &pin.commit, &pin.kept)?;
    let repo = work.path().join("repo");
    let old = build_old_commits(
        git,
        root,
        work.path(),
        &repo,
        &OldRelease {
            upstream: &pin.upstream,
            commit: &settling.from,
            kept: &settling.from_kept,
            patches: &made.patches[at..],
        },
    )?;
    let new_tree = start_new_branch(git, &repo, &new_base)?;
    for done in &made.patches[..at] {
        apply_exact(git, &repo, done, &done.path(root), false)?;
    }
    let before = head(git, &repo)?;
    committed(git, &repo, &copy, &ours, settling.patch.as_str())?;
    let settled_diff = diff_from(git, &repo, &before)?;

    let mut kept_patches = made.patches[..at].to_vec();
    let mut dropped = Vec::new();
    if settled_diff.is_empty() {
        remove_patch(root, &settling.patch)?;
        dropped.push(settling.patch);
    } else {
        write_patch(root, &settling.patch, &settled_diff)?;
        kept_patches.push(settling.patch);
    }
    let step = PickStep {
        pin,
        new_base: &new_base,
        new_tree,
        kept_patches,
        dropped,
        from: &settling.from,
        from_kept: &settling.from_kept,
    };
    pick_patches(git, root, &repo, step, &old[1..])
}

struct PickStep<'a> {
    pin: &'a Pin,
    new_base: &'a Path,
    new_tree: Tree,
    kept_patches: Vec<PatchName>,
    dropped: Vec<PatchName>,
    from: &'a Commit,
    from_kept: &'a [Kept],
}

fn pick_patches(
    git: &Git,
    root: &Path,
    repo: &Path,
    mut step: PickStep<'_>,
    remaining: &[(PatchName, String)],
) -> Result<Synced, Error> {
    let copy = root.join(COPY);
    for (index, (name, commit)) in remaining.iter().enumerate() {
        match cherry_pick(git, repo, commit)? {
            CherryPick::Clean { diff } if diff.is_empty() => {
                remove_patch(root, name)?;
                step.dropped.push(name.clone());
            }
            CherryPick::Clean { diff } => {
                write_patch(root, name, &diff)?;
                step.kept_patches.push(name.clone());
            }
            CherryPick::Conflicted(files) => {
                let mut patches = step.kept_patches;
                patches.push(name.clone());
                patches.extend(remaining[index + 1..].iter().map(|(later, _)| later.clone()));
                replace(&copy, &tracked(git, root)?, repo)?;
                Record {
                    release: step.pin.release.clone(),
                    commit: step.pin.commit.clone(),
                    tree: step.new_tree,
                    kept: step.pin.kept.clone(),
                    patches,
                    settling: Some(Settling {
                        patch: name.clone(),
                        from: step.from.clone(),
                        from_kept: step.from_kept.to_vec(),
                        files: files.clone(),
                    }),
                }
                .write(root)?;
                return Ok(Synced::Unsettled { patch: name.clone(), files });
            }
        }
    }
    replace_stamped(&copy, &tracked(git, root)?, repo, step.new_base)?;
    Record {
        release: step.pin.release.clone(),
        commit: step.pin.commit.clone(),
        tree: step.new_tree,
        kept: step.pin.kept.clone(),
        patches: step.kept_patches,
        settling: None,
    }
    .write(root)?;
    Ok(Synced::Brought { moved: pin_versions(root)?, dropped: step.dropped })
}

fn cut_checked(
    git: &Git,
    root: &Path,
    work: &Path,
    upstream: &str,
    commit: &Commit,
    kept: &[Kept],
) -> Result<PathBuf, Error> {
    let (checkout, new_base) = (work.join("new_upstream"), work.join("new_base"));
    fetch(git, upstream, commit, &checkout)?;
    cut(&checkout, &new_base, kept)?;
    let left_out = ignored(git, root, &files_under(&new_base)?, &work.join("listed"))?;
    if !left_out.is_empty() {
        return Err(Error::Ignored(left_out));
    }
    Ok(new_base)
}

fn check_settled(copy: &Path, ours: &[CopyPath], settled_files: &[CopyPath]) -> Result<(), Error> {
    let mut to_check = ours.to_vec();
    for file in settled_files {
        if !to_check.contains(file) {
            to_check.push(file.clone());
        }
    }
    let still = marked_in(copy, &to_check)?;
    if !still.is_empty() {
        return Err(Error::StillMarked(still));
    }
    Ok(())
}

struct OldRelease<'a> {
    upstream: &'a str,
    commit: &'a Commit,
    kept: &'a [Kept],
    patches: &'a [PatchName],
}

fn build_old_commits(
    git: &Git,
    root: &Path,
    work: &Path,
    repo: &Path,
    old: &OldRelease<'_>,
) -> Result<Vec<(PatchName, String)>, Error> {
    let (checkout, old_base) = (work.join("old_upstream"), work.join("old_base"));
    fetch(git, old.upstream, old.commit, &checkout)?;
    cut(&checkout, &old_base, old.kept)?;
    init(git, repo)?;
    committed(git, repo, &old_base, &files_under(&old_base)?, "Old")?;
    let mut commits = Vec::with_capacity(old.patches.len());
    for name in old.patches {
        let sha = apply_for_old(git, repo, name, &name.path(root))?;
        commits.push((name.clone(), sha));
    }
    Ok(commits)
}

fn start_new_branch(git: &Git, repo: &Path, new_base: &Path) -> Result<Tree, Error> {
    git.run(Settings::Isolated, repo, ["checkout", "-q", "--orphan", "new"])?;
    committed(git, repo, new_base, &files_under(new_base)?, "New")?;
    tree_of_dir(new_base)
}
