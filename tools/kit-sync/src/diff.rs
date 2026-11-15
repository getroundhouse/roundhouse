//! Our changes to the copy, as a patch to offer upstream.

use std::fs;
use std::path::Path;

use crate::copy::{COPY, tracked};
use crate::cut::cut;
use crate::error::{At, Error};
use crate::files::files_under;
use crate::git::{Git, Settings};
use crate::record::Record;
use crate::scratch::{Notices, committed};
use crate::upstream::fetch;

/// Our changes to the copy in the repository at `root`, or to the files `paths` name from the copy, as a patch against
/// the release it's on, from the root of upstream's repository, without the lines that say a file was changed.
pub fn diff(git: &Git, root: &Path, upstream: &str, paths: &[String]) -> Result<String, Error> {
    let made = Record::read(root)?.ok_or(Error::NoCopy)?;
    let scratch = root.join("target");
    fs::create_dir_all(&scratch).at(&scratch)?;
    let work = tempfile::Builder::new().prefix("kit-sync-").tempdir_in(&scratch).at(&scratch)?;
    let (checkout, base, repo) = (work.path().join("upstream"), work.path().join("base"), work.path().join("diff"));
    fetch(git, upstream, &made.commit, &checkout)?;
    cut(&checkout, &base, &made.kept)?;
    fs::create_dir(&repo).at(&repo)?;
    git.run(Settings::Isolated, &repo, ["init", "-q"])?;
    committed(git, &repo, &base, &files_under(&base)?, "Upstream", Notices::Taken)?;
    committed(git, &repo, &root.join(COPY), &tracked(git, root)?, "Ours", Notices::Taken)?;
    let args = ["diff", "--binary", "HEAD~", "HEAD", "--"].into_iter().chain(paths.iter().map(String::as_str));
    git.run(Settings::Isolated, &repo, args)
}
