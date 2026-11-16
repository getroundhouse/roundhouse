//! The release of upstream's that the copy is pinned to, and what the copy keeps of it.

use std::fmt;

use serde::Deserialize;

use crate::files::CopyPath;

/// Upstream's repository, which the copy is made from.
pub const UPSTREAM: &str = "https://github.com/longbridge/gpui-kit.git";

// renovate: datasource=github-tags depName=longbridge/gpui-kit
const RELEASE: &str = "v0.10.0";
/// The commit [`RELEASE`] tags. A tag can be moved, so a copy is made only from the commit it was checked to be.
const COMMIT: &str = "f697c3133d55c64fa8a5b67926147fb3dadd276d";

/// What the copy keeps of upstream's repository: gpui-kit and every crate under it that's upstream's, then the
/// workspace manifest the crates take their versions, dependencies and lints from, rustfmt's settings for upstream's
/// style, and the license. Then what the kept crates' code reads from elsewhere in the repository, which [`sync`]
/// refuses to leave out. Then, after a `!`, what it leaves out of those crates: gpui-base's examples, which are
/// programs of their own that read files from upstream's other crates, one a font under a license of its own, and
/// which upstream leaves out of the crate it publishes too.
///
/// [`sync`]: crate::sync
const KEPT: &[&str] = &[
    "crates/kit",
    "crates/base",
    "crates/component",
    "crates/component-macros",
    "crates/assets",
    "Cargo.toml",
    ".rustfmt.toml",
    "LICENSE-APACHE",
    "README.md",
    "examples/fixtures/test.md",
    "themes/aurora.json",
    "!crates/base/examples",
];

/// A release of upstream's to make the copy from, and what to keep of it.
#[derive(Clone, Debug)]
pub struct Pin {
    /// Where upstream's repository is, which git can fetch from.
    pub upstream: String,
    pub release: Release,
    /// The commit `release` tags, which [`crate::sync`] checks.
    pub commit: Commit,
    pub kept: Vec<Kept>,
}

impl Pin {
    /// The release this repository's copy is pinned to.
    pub fn ours() -> Self {
        Self {
            upstream: UPSTREAM.to_string(),
            release: Release::new(RELEASE),
            commit: Commit::new(COMMIT),
            kept: KEPT.iter().map(|path| Kept::from(*path)).collect(),
        }
    }
}

/// A release's tag upstream, such as `v0.9.0`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Release(String);

impl Release {
    pub fn new(tag: impl Into<String>) -> Self {
        Self(tag.into())
    }
}

impl fmt::Display for Release {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A commit's full hash.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Commit(String);

impl Commit {
    pub fn new(hash: impl Into<String>) -> Self {
        Self(hash.into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Commit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A path of upstream's the copy keeps whole, or one it leaves out of a folder it keeps. Written, as in [`KEPT`] and
/// the record, one left out starts with `!`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kept {
    Whole(CopyPath),
    LeftOut(CopyPath),
}

impl From<&str> for Kept {
    fn from(written: &str) -> Self {
        match written.strip_prefix('!') {
            Some(path) => Self::LeftOut(CopyPath::new(path)),
            None => Self::Whole(CopyPath::new(written)),
        }
    }
}

impl fmt::Display for Kept {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Whole(path) => write!(f, "{path}"),
            Self::LeftOut(path) => write!(f, "!{path}"),
        }
    }
}
