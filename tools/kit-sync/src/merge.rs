//! Upstream's files at a release, put into the copy, merged with our changes to the release it was made from.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::copy::{COPY, ignored, tracked};
use crate::cut::cut;
use crate::error::{At, Error};
use crate::files::{CopyPath, files_under, put, remove};
use crate::git::{Git, Settings};
use crate::pin::Pin;
use crate::record::Record;
use crate::scratch::{Notices, committed};
use crate::upstream::fetch;

/// Puts upstream's files at `pin`'s release into the copy, merged with our changes to the release it was `made`
/// from, using `work` for upstream's checkouts and the merge. Returns the files git left with a conflict.
pub(crate) fn brought(
    git: &Git,
    root: &Path,
    pin: &Pin,
    made: Option<&Record>,
    work: &Path,
) -> Result<Vec<CopyPath>, Error> {
    let copy = root.join(COPY);
    let new = work.join("new");
    fetch(git, &pin.upstream, &pin.commit, &work.join("upstream"))?;
    cut(&work.join("upstream"), &new, &pin.kept)?;
    let files = files_under(&new)?;
    let left_out = ignored(git, root, &files, &work.join("listed"))?;
    if !left_out.is_empty() {
        return Err(Error::Ignored(left_out));
    }
    let Some(made) = made else {
        let theirs = if copy.exists() { tracked(git, root)? } else { Vec::new() };
        if !theirs.is_empty() {
            return Err(Error::Unrecorded(theirs.into_iter().take(5).collect()));
        }
        return put(&new, &files, &copy).map(|()| Vec::new());
    };
    fetch(git, &pin.upstream, &made.commit, &work.join("before"))?;
    cut(&work.join("before"), &work.join("base"), &made.kept)?;
    merged(git, &copy, &tracked(git, root)?, work)
}

/// Merges, in a repository of its own in `work`, what upstream changed from the cut in `base` to the cut in `new`
/// with what we changed in `copy`, whose files are `ours`, and puts the result in `copy`.
fn merged(git: &Git, copy: &Path, ours: &[CopyPath], work: &Path) -> Result<Vec<CopyPath>, Error> {
    let repo = work.join("merge");
    fs::create_dir(&repo).at(&repo)?;
    let (base, new) = (work.join("base"), work.join("new"));
    git.run(Settings::Isolated, &repo, ["init", "-q", "-b", "base"])?;
    let upstream_before = "Upstream, at the release the copy was made from";
    committed(git, &repo, &base, &files_under(&base)?, upstream_before, Notices::Kept)?;
    git.run(Settings::Isolated, &repo, ["checkout", "-q", "-b", "upstream"])?;
    let upstream_now = "Upstream, at the release to bring the copy to";
    committed(git, &repo, &new, &files_under(&new)?, upstream_now, Notices::Kept)?;
    git.run(Settings::Isolated, &repo, ["checkout", "-q", "-b", "ours", "base"])?;
    committed(git, &repo, copy, ours, "The copy, with our changes", Notices::Kept)?;

    let merging =
        git.output(Settings::Isolated, &repo, &["merge", "-q", "--no-edit", "upstream"].map(Into::into), None)?;
    let unmerged = git.run(Settings::Isolated, &repo, ["diff", "-z", "--name-only", "--diff-filter=U"])?;
    let conflicts: Vec<CopyPath> = unmerged.split('\0').filter(|path| !path.is_empty()).map(CopyPath::new).collect();
    if !merging.status.success() && conflicts.is_empty() {
        return Err(Error::Unmerged(String::from_utf8_lossy(&merging.stderr).trim().to_string()));
    }

    let result = files_under(&repo)?;
    let kept: BTreeSet<&CopyPath> = result.iter().collect();
    for gone in ours.iter().filter(|path| !kept.contains(path)) {
        remove(copy, gone)?;
    }
    put(&repo, &result, copy)?;
    Ok(conflicts)
}
