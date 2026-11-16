//! The tool's own temporary repositories, in which upstream's releases, our patches and the copy are committed,
//! applied and cherry-picked without the person's git settings.

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use tempfile::TempDir;

use crate::error::{At, Error};
use crate::files::{CopyPath, put};
use crate::git::{Git, Settings};
use crate::notice::without_notices;
use crate::patches::PatchName;

/// How cherry-picking a patch commit went.
pub(crate) enum CherryPick {
    /// Applied cleanly, with `diff` against the commit before it (`""` when upstream already has the change).
    Clean { diff: String },
    /// Stopped with conflicts in these files.
    Conflicted(Vec<CopyPath>),
}

/// A fresh temporary folder outside the repository, so checking a read-only checkout writes nothing into it.
pub(crate) fn work_dir() -> Result<TempDir, Error> {
    tempfile::Builder::new().prefix("kit-sync-").tempdir().at(&std::env::temp_dir())
}

/// Initializes an isolated git repository at `repo` on branch `base`.
pub(crate) fn init(git: &Git, repo: &Path) -> Result<(), Error> {
    fs::create_dir_all(repo).at(repo)?;
    git.run(Settings::Isolated, repo, ["init", "-q", "-b", "base"]).map(|_| ())
}

/// Makes `files` from `source`, stripped of notice lines, all that's on `repo`'s branch, as a commit.
pub(crate) fn committed(git: &Git, repo: &Path, source: &Path, files: &[CopyPath], message: &str) -> Result<(), Error> {
    git.run(Settings::Isolated, repo, ["rm", "-rfq", "--ignore-unmatch", "."])?;
    put(source, files, repo)?;
    for path in files.iter().map(|path| path.under(repo)) {
        if fs::symlink_metadata(&path).at(&path)?.is_file()
            && let Some(text) = without_notices(&fs::read(&path).at(&path)?)
        {
            fs::write(&path, text).at(&path)?;
        }
    }
    // Forced, since what upstream keeps in git is what's committed here, whatever its own ignore files say.
    git.run(Settings::Isolated, repo, ["add", "-A", "--force"])?;
    git.run(Settings::Isolated, repo, ["commit", "-q", "--allow-empty", "-m", message]).map(|_| ())
}

/// The hash of `repo`'s `HEAD` commit.
pub(crate) fn head(git: &Git, repo: &Path) -> Result<String, Error> {
    Ok(git.run(Settings::Isolated, repo, ["rev-parse", "HEAD"])?.trim().to_string())
}

/// The patch from `rev` to `HEAD` in `repo`, binary changes included.
pub(crate) fn diff_from(git: &Git, repo: &Path, rev: &str) -> Result<String, Error> {
    git.run(Settings::Isolated, repo, ["diff", "--binary", rev, "HEAD"])
}

/// Applies `patch_file` (or reverses it when `reverse` is set) onto `repo`'s index and working tree with no fuzz, and
/// commits it when applying forward.
pub(crate) fn apply_exact(
    git: &Git,
    repo: &Path,
    name: &PatchName,
    patch_file: &Path,
    reverse: bool,
) -> Result<(), Error> {
    let mut args: Vec<OsString> = ["apply", "--index", "--whitespace=nowarn"].map(OsString::from).into();
    if reverse {
        args.push("-R".into());
    }
    args.push(patch_file.as_os_str().to_os_string());
    let output = git.output(Settings::Isolated, repo, &args, None)?;
    if !output.status.success() {
        return Err(Error::PatchDrifted(name.clone()));
    }
    if !reverse {
        git.run(Settings::Isolated, repo, ["commit", "-q", "--allow-empty", "-m", name.as_str()])?;
    }
    Ok(())
}

/// Applies `patch_file` onto `repo` to build an old commit to cherry-pick from, using 3-way fallback when an earlier
/// patch in the stack was already moved onto the new release, and returns the commit's hash.
pub(crate) fn apply_for_old(git: &Git, repo: &Path, name: &PatchName, patch_file: &Path) -> Result<String, Error> {
    let exact: Vec<OsString> =
        ["apply".into(), "--index".into(), "--whitespace=nowarn".into(), patch_file.as_os_str().to_os_string()].into();
    if !git.output(Settings::Isolated, repo, &exact, None)?.status.success() {
        let three_way = ["apply", "-3", "--whitespace=nowarn", &patch_file.to_string_lossy()];
        git.run(Settings::Isolated, repo, three_way)?;
        git.run(Settings::Isolated, repo, ["add", "-A", "--force"])?;
    }
    git.run(Settings::Isolated, repo, ["commit", "-q", "--allow-empty", "-m", name.as_str()])?;
    head(git, repo)
}

/// Cherry-picks `commit` onto `repo`'s `HEAD`, keeping an empty commit when upstream already made the same change.
pub(crate) fn cherry_pick(git: &Git, repo: &Path, commit: &str) -> Result<CherryPick, Error> {
    let args = ["cherry-pick", "--keep-redundant-commits", commit].map(OsString::from);
    let output = git.output(Settings::Isolated, repo, &args, None)?;
    if output.status.success() {
        return Ok(CherryPick::Clean { diff: diff_from(git, repo, "HEAD~")? });
    }
    let unmerged = git.run(Settings::Isolated, repo, ["diff", "-z", "--name-only", "--diff-filter=U"])?;
    let conflicts: Vec<CopyPath> = unmerged.split('\0').filter(|path| !path.is_empty()).map(CopyPath::new).collect();
    if conflicts.is_empty() {
        return Err(Error::Unmerged(String::from_utf8_lossy(&output.stderr).trim().to_string()));
    }
    Ok(CherryPick::Conflicted(conflicts))
}
