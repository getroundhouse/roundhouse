//! Checks offline that the copy is on the pinned release, with only the recorded patches on top and notices on every
//! file that differs from upstream.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::copy::{COPY, marked_in, tracked};
use crate::digest::tree_of_sources;
use crate::error::Error;
use crate::files::{CopyPath, files_under};
use crate::git::{Git, Settings};
use crate::notice::wrong_notices;
use crate::patches::{check_patch_files, touched_by_patches};
use crate::pin::Pin;
use crate::record::Record;
use crate::scratch::{apply_exact, committed, init, work_dir};

/// The temporary repository [`verify_against_record`] leaves at the unpatched release for the files our patches touch.
pub(crate) struct Verified {
    pub(crate) work: TempDir,
    pub(crate) repo: PathBuf,
    pub(crate) touched: BTreeSet<CopyPath>,
}

/// Checks offline that `root`'s copy of the kit is on `pin`, with no unsettled conflicts, every patch in
/// [`crate::PATCHES`] matching [`crate::RECORD`], no unpatched edits in the copy, and notices on the changed files.
pub fn verify(git: &Git, root: &Path, pin: &Pin) -> Result<(), Error> {
    let made = Record::read(root)?.ok_or(Error::NoCopy)?;
    if let Some(settling) = &made.settling {
        return Err(Error::Unsettled { patch: settling.patch.clone(), files: settling.files.clone() });
    }
    if (&made.release, &made.commit, &made.kept) != (&pin.release, &pin.commit, &pin.kept) {
        return Err(Error::NotAtPin { copy: made.release, pinned: pin.release.clone() });
    }
    verify_against_record(git, root, &made).map(|_| ())
}

/// Checks `root`'s copy and patch files against `made`, reversing only the files the patches touch so checking thousands
/// of untouched files writes almost nothing to disk.
pub(crate) fn verify_against_record(git: &Git, root: &Path, made: &Record) -> Result<Verified, Error> {
    let copy = root.join(COPY);
    let ours = tracked(git, root)?;
    let still = marked_in(&copy, &ours)?;
    if !still.is_empty() {
        return Err(Error::StillMarked(still));
    }
    check_patch_files(root, &made.patches)?;
    let touched = touched_by_patches(root, &made.patches)?;
    let ours_touched: Vec<CopyPath> = ours.iter().filter(|path| touched.contains(path)).cloned().collect();

    let work = work_dir()?;
    let repo = work.path().join("verify");
    init(git, &repo)?;
    committed(git, &repo, &copy, &ours_touched, "Touched")?;
    for name in made.patches.iter().rev() {
        apply_exact(git, &repo, name, &name.path(root), true)?;
    }
    git.run(Settings::Isolated, &repo, ["commit", "-q", "--allow-empty", "-m", "Base"])?;

    let recovered = files_under(&repo)?;
    let mut sources: BTreeMap<&CopyPath, &Path> =
        ours.iter().filter(|path| !touched.contains(*path)).map(|path| (path, copy.as_path())).collect();
    for path in &recovered {
        sources.insert(path, repo.as_path());
    }
    if tree_of_sources(&sources)? != made.tree {
        return Err(Error::UnpatchedChanges);
    }

    let untouched: Vec<CopyPath> = ours.iter().filter(|path| !touched.contains(*path)).cloned().collect();
    let mut wrong = wrong_notices(&copy, &untouched, &copy)?;
    wrong.extend(wrong_notices(&copy, &ours_touched, &repo)?);
    wrong.sort();
    if !wrong.is_empty() {
        return Err(Error::WrongNotices(wrong));
    }
    Ok(Verified { work, repo, touched })
}
