//! Recording changes to the copy in [`crate::PATCHES`], and taking a patch out of the copy.

use std::fs;
use std::path::Path;

use crate::copy::{COPY, marked_in, replace_stamped, tracked};
use crate::cut::cut;
use crate::error::{At, Error};
use crate::files::{CopyPath, files_under, put};
use crate::git::{Git, Settings};
use crate::patches::{PatchName, check_patch_files, remove_patch, write_patch};
use crate::record::Record;
use crate::scratch::{CherryPick, apply_exact, cherry_pick, committed, diff_from, head, init, work_dir};
use crate::upstream::fetch;
use crate::verify::{Verified, verify_against_record};

/// How recording a patch left [`crate::PATCHES`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Patched {
    /// Wrote `<name>.patch` and added `<name>` to [`crate::RECORD`].
    Added,
    /// Updated `<name>.patch` in place.
    Updated,
    /// `<name>.patch` already held what's in the copy.
    Already,
}

/// Records our changes in `root`'s copy into `<name>.patch`, adding `name` to [`crate::RECORD`] when it's new or
/// updating it in place when it's already listed, and stamps notices on the changed files.
pub fn patch(git: &Git, root: &Path, upstream: &str, name: &PatchName) -> Result<Patched, Error> {
    let made = Record::read(root)?.ok_or(Error::NoCopy)?;
    if let Some(settling) = &made.settling {
        return Err(Error::Unsettled { patch: settling.patch.clone(), files: settling.files.clone() });
    }
    let copy = root.join(COPY);
    let ours = tracked(git, root)?;
    let still = marked_in(&copy, &ours)?;
    if !still.is_empty() {
        return Err(Error::StillMarked(still));
    }
    check_patch_files(root, &made.patches)?;

    let work = work_dir()?;
    let (checkout, base, repo) = (work.path().join("upstream"), work.path().join("base"), work.path().join("repo"));
    fetch(git, upstream, &made.commit, &checkout)?;
    cut(&checkout, &base, &made.kept)?;
    init(git, &repo)?;
    committed(git, &repo, &base, &files_under(&base)?, "Base")?;

    let context = PatchContext { git, root, repo: &repo, base: &base, copy: &copy, ours: &ours };
    match made.patches.iter().position(|existing| existing == name) {
        None => add_patch(&context, made, name),
        Some(at) => update_patch(&context, &made, name, at),
    }
}

/// Takes `name` out of `root`'s copy and [`crate::RECORD`], deletes `<name>.patch`, and refreshes later patches.
pub fn drop_patch(git: &Git, root: &Path, name: &PatchName) -> Result<(), Error> {
    let mut made = Record::read(root)?.ok_or(Error::NoCopy)?;
    if let Some(settling) = &made.settling {
        return Err(Error::Unsettled { patch: settling.patch.clone(), files: settling.files.clone() });
    }
    let at =
        made.patches.iter().position(|existing| existing == name).ok_or_else(|| Error::NoSuchPatch(name.clone()))?;
    let Verified { work, repo, touched } = verify_against_record(git, root, &made)?;
    let base = work.path().join("base");
    fs::create_dir_all(&base).at(&base)?;
    put(&repo, &files_under(&repo)?, &base)?;

    for before in &made.patches[..at] {
        apply_exact(git, &repo, before, &before.path(root), false)?;
    }
    let before_dropped = head(git, &repo)?;
    apply_exact(git, &repo, name, &name.path(root), false)?;
    let later = apply_commits(git, root, &repo, &made.patches[at + 1..])?;

    git.run(Settings::Isolated, &repo, ["checkout", "-q", "-b", "dropped", &before_dropped])?;
    let refreshed = replay_later(git, &repo, &later, |later_name| Error::PatchDependedOn {
        dropped: name.clone(),
        by: later_name,
    })?;
    for (later_name, diff) in refreshed {
        write_patch(root, &later_name, &diff)?;
    }
    remove_patch(root, name)?;
    made.patches.remove(at);
    made.write(root)?;
    let ours_touched: Vec<CopyPath> = tracked(git, root)?.into_iter().filter(|path| touched.contains(path)).collect();
    replace_stamped(&root.join(COPY), &ours_touched, &repo, &base)
}

struct PatchContext<'a> {
    git: &'a Git,
    root: &'a Path,
    repo: &'a Path,
    base: &'a Path,
    copy: &'a Path,
    ours: &'a [CopyPath],
}

fn add_patch(cx: &PatchContext<'_>, mut made: Record, name: &PatchName) -> Result<Patched, Error> {
    for existing in &made.patches {
        apply_exact(cx.git, cx.repo, existing, &existing.path(cx.root), false)?;
    }
    let before = head(cx.git, cx.repo)?;
    committed(cx.git, cx.repo, cx.copy, cx.ours, name.as_str())?;
    let diff = diff_from(cx.git, cx.repo, &before)?;
    if diff.is_empty() {
        return Err(Error::NothingChanged);
    }
    write_patch(cx.root, name, &diff)?;
    made.patches.push(name.clone());
    made.write(cx.root)?;
    replace_stamped(cx.copy, cx.ours, cx.repo, cx.base)?;
    Ok(Patched::Added)
}

fn update_patch(cx: &PatchContext<'_>, made: &Record, name: &PatchName, at: usize) -> Result<Patched, Error> {
    for existing in &made.patches[..at] {
        apply_exact(cx.git, cx.repo, existing, &existing.path(cx.root), false)?;
    }
    let before = head(cx.git, cx.repo)?;
    apply_exact(cx.git, cx.repo, name, &name.path(cx.root), false)?;
    let old_target = head(cx.git, cx.repo)?;
    let later = apply_commits(cx.git, cx.root, cx.repo, &made.patches[at + 1..])?;
    let old_top = head(cx.git, cx.repo)?;

    committed(cx.git, cx.repo, cx.copy, cx.ours, "Working")?;
    if diff_from(cx.git, cx.repo, &old_top)?.is_empty() {
        replace_stamped(cx.copy, cx.ours, cx.repo, cx.base)?;
        return Ok(Patched::Already);
    }
    let working = head(cx.git, cx.repo)?;
    if later.is_empty() {
        let diff = diff_from(cx.git, cx.repo, &before)?;
        if diff.is_empty() {
            return Err(Error::PatchEmpty(name.clone()));
        }
        write_patch(cx.root, name, &diff)?;
        replace_stamped(cx.copy, cx.ours, cx.repo, cx.base)?;
        return Ok(Patched::Updated);
    }

    cx.git.run(Settings::Isolated, cx.repo, ["checkout", "-q", "-b", "updated", &old_target])?;
    if matches!(cherry_pick(cx.git, cx.repo, &working)?, CherryPick::Conflicted(_)) {
        return Err(Error::EntangledWithLaterPatch(name.clone()));
    }
    let diff = diff_from(cx.git, cx.repo, &before)?;
    if diff.is_empty() {
        return Err(Error::PatchEmpty(name.clone()));
    }
    let refreshed = replay_later(cx.git, cx.repo, &later, |later_name| Error::ConflictsWithLaterPatch {
        updated: name.clone(),
        later: later_name,
    })?;
    write_patch(cx.root, name, &diff)?;
    for (later_name, later_diff) in refreshed {
        write_patch(cx.root, &later_name, &later_diff)?;
    }
    replace_stamped(cx.copy, cx.ours, cx.repo, cx.base)?;
    Ok(Patched::Updated)
}

fn apply_commits(git: &Git, root: &Path, repo: &Path, names: &[PatchName]) -> Result<Vec<(PatchName, String)>, Error> {
    let mut commits = Vec::with_capacity(names.len());
    for name in names {
        apply_exact(git, repo, name, &name.path(root), false)?;
        commits.push((name.clone(), head(git, repo)?));
    }
    Ok(commits)
}

fn replay_later(
    git: &Git,
    repo: &Path,
    later: &[(PatchName, String)],
    on_conflict: impl Fn(PatchName) -> Error,
) -> Result<Vec<(PatchName, String)>, Error> {
    let mut refreshed = Vec::with_capacity(later.len());
    for (name, commit) in later {
        match cherry_pick(git, repo, commit)? {
            CherryPick::Conflicted(_) => return Err(on_conflict(name.clone())),
            CherryPick::Clean { diff } if diff.is_empty() => return Err(Error::PatchEmpty(name.clone())),
            CherryPick::Clean { diff } => refreshed.push((name.clone(), diff)),
        }
    }
    Ok(refreshed)
}
