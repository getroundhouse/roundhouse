//! Upstream's repository: which commit a release tags, and its files at a commit.

use std::fs;
use std::path::Path;

use crate::error::{At, Error};
use crate::git::{Git, Settings};
use crate::pin::{Commit, Release};

/// The commit `release` tags upstream, or `None` when there's no such tag, asked from `dir`.
pub(crate) fn tagged(git: &Git, dir: &Path, upstream: &str, release: &Release) -> Result<Option<Commit>, Error> {
    let tag = format!("refs/tags/{release}");
    // An annotated tag is listed as itself and, with `^{}`, as the commit it tags.
    let peeled = format!("{tag}^{{}}");
    let listed = git.run(Settings::Persons, dir, ["ls-remote", upstream, &tag, &peeled])?;
    let commit = |name: &str| {
        (listed.lines())
            .filter_map(|line| line.split_once('\t'))
            .find_map(|(commit, listed)| (listed == name).then(|| Commit::new(commit)))
    };
    Ok(commit(&peeled).or_else(|| commit(&tag)))
}

/// Upstream's files at `commit`, checked out in `into`, without its history.
pub(crate) fn fetch(git: &Git, upstream: &str, commit: &Commit, into: &Path) -> Result<(), Error> {
    fs::create_dir_all(into).at(into)?;
    git.run(Settings::Isolated, into, ["init", "-q"])?;
    git.run(Settings::Persons, into, ["fetch", "-q", "--depth", "1", upstream, commit.as_str()])?;
    git.run(Settings::Isolated, into, ["checkout", "-q", "FETCH_HEAD"])?;
    Ok(())
}
