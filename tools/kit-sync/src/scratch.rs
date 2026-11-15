//! The tool's own repositories, in which upstream's releases and the copy are committed to be merged or compared.

use std::fs;
use std::path::Path;

use crate::error::{At, Error};
use crate::files::{CopyPath, put};
use crate::git::{Git, Settings};
use crate::notice::without_notices;

/// Whether the lines that say a file was changed stay in what's committed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Notices {
    Kept,
    /// Left out, for a patch to offer upstream.
    Taken,
}

/// Makes `files` from `source` all that's on `repo`'s branch, as a commit.
pub(crate) fn committed(
    git: &Git,
    repo: &Path,
    source: &Path,
    files: &[CopyPath],
    message: &str,
    notices: Notices,
) -> Result<(), Error> {
    git.run(Settings::Isolated, repo, ["rm", "-rq", "--ignore-unmatch", "."])?;
    put(source, files, repo)?;
    if notices == Notices::Taken {
        for path in files.iter().map(|path| path.under(repo)) {
            if fs::symlink_metadata(&path).at(&path)?.is_file()
                && let Some(text) = without_notices(&fs::read(&path).at(&path)?)
            {
                fs::write(&path, text).at(&path)?;
            }
        }
    }
    // Forced, since what upstream keeps in git is what's committed here, whatever its own ignore files say.
    git.run(Settings::Isolated, repo, ["add", "-A", "--force"])?;
    git.run(Settings::Isolated, repo, ["commit", "-q", "--allow-empty", "-m", message])?;
    Ok(())
}
