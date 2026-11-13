//! The repository the guide is read from, and a path in it.

use std::fmt;
use std::path::{Path, PathBuf};

/// Where the guide is, from the repository's root.
pub(crate) const GUIDE_DIR: &str = "docs/guide";

/// Where the repository is read on the web, for a link to one of its files that isn't a page of the guide.
const REPO_URL: &str = "https://github.com/getroundhouse/roundhouse";

/// The repository the guide is read from. Its pages are in `docs/guide`, and what they link to and show can be
/// anywhere in it.
pub struct Source {
    repo: PathBuf,
}

impl Source {
    pub fn in_repo(repo: &Path) -> Self {
        Self { repo: repo.to_path_buf() }
    }

    /// Where `path` is on this machine.
    pub(crate) fn on_disk(&self, path: &RepoPath) -> PathBuf {
        self.repo.join(&path.0)
    }
}

/// A path from the repository's root, with forward slashes and no `.` or `..`, such as `docs/spec/screens/home.png`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RepoPath(String);

impl RepoPath {
    /// The path of these names, one inside the next.
    pub(crate) fn of(names: &[&str]) -> Self {
        Self(names.join("/"))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// Where the file is read on GitHub, which is where a link to it from a published page goes.
    pub(crate) fn on_github(&self) -> String {
        format!("{REPO_URL}/blob/main/{}", self.0)
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
