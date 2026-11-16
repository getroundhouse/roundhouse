//! The copy as this repository has it: which of its files git keeps, which it would ignore, and replacing them.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs::{self, File};
use std::path::Path;

use crate::error::{At, Error};
use crate::files::{CopyPath, files_under, marked, put, remove, there};
use crate::git::{Git, Settings};
use crate::notice::stamp_notices;

/// Where the copy is, from the root of this repository.
pub const COPY: &str = "third_party/gpui-kit/vendor";

/// The copy's files that git keeps or would keep, so what's been built in it, such as a `target` folder, is left
/// out, and so is what's been deleted.
pub(crate) fn tracked(git: &Git, root: &Path) -> Result<Vec<CopyPath>, Error> {
    let args = ["ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", COPY];
    let listed = git.run(Settings::Persons, root, args)?;
    let prefix = format!("{COPY}/");
    let paths: BTreeSet<_> = (listed.split('\0'))
        .filter_map(|path| path.strip_prefix(&prefix))
        .map(CopyPath::new)
        .filter(|path| there(&path.under(&root.join(COPY))))
        .collect();
    Ok(paths.into_iter().collect())
}

/// Which of `files` in `copy` still have git's conflict markers.
pub(crate) fn marked_in(copy: &Path, files: &[CopyPath]) -> Result<Vec<CopyPath>, Error> {
    let mut still = Vec::new();
    for path in files {
        if marked(&path.under(copy))? {
            still.push(path.clone());
        }
    }
    Ok(still)
}

/// Which of the copy's `files` this repository's ignore files would keep out of git. `scratch` is a file to write
/// the list to, for git to read.
pub(crate) fn ignored(git: &Git, root: &Path, files: &[CopyPath], scratch: &Path) -> Result<Vec<CopyPath>, Error> {
    let listed: String = files.iter().flat_map(|path| [COPY, "/", path.as_str(), "\0"]).collect();
    fs::write(scratch, listed).at(scratch)?;
    let args: Vec<OsString> = ["check-ignore", "--no-index", "--stdin", "-z"].map(OsString::from).into();
    let output = git.output(Settings::Persons, root, &args, Some(File::open(scratch).at(scratch)?))?;
    // It exits with 1 when nothing is ignored.
    if output.status.code().is_none_or(|code| code > 1) {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(Error::Git { command: args.join(" ".as_ref()).to_string_lossy().into_owned(), stderr });
    }
    let prefix = format!("{COPY}/");
    Ok((String::from_utf8_lossy(&output.stdout).split('\0'))
        .filter_map(|path| path.strip_prefix(&prefix))
        .map(CopyPath::new)
        .collect())
}

/// Stamps notices in `from` against `base`, then replaces `ours` in `copy` with what's in `from`.
pub(crate) fn replace_stamped(copy: &Path, ours: &[CopyPath], from: &Path, base: &Path) -> Result<(), Error> {
    stamp_notices(from, base)?;
    replace(copy, ours, from)
}

/// Replaces `ours` in `copy` with what's in `from`, leaving ignored files in `copy` alone.
pub(crate) fn replace(copy: &Path, ours: &[CopyPath], from: &Path) -> Result<(), Error> {
    let result = files_under(from)?;
    let kept: BTreeSet<&CopyPath> = result.iter().collect();
    for gone in ours.iter().filter(|path| !kept.contains(path)) {
        remove(copy, gone)?;
    }
    put(from, &result, copy)
}
