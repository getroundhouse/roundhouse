//! Why the copy was left as it was.

use std::fmt::Display;
use std::io;
use std::path::{Path, PathBuf};

use crate::copy::COPY;
use crate::files::CopyPath;
use crate::pin::{Commit, Release};
use crate::reads::Read;
use crate::record::RECORD;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("upstream's {release} tags {}, not {pinned}: set COMMIT to the commit of the release meant", tagged.as_ref().map_or_else(|| "nothing".to_string(), ToString::to_string))]
    TagElsewhere { release: Release, tagged: Option<Commit>, pinned: Commit },
    #[error("these still have git's conflict markers, so settle them first: {}", listed(.0))]
    StillMarked(Vec<CopyPath>),
    #[error("upstream has no {0} here, so what the copy keeps has to change with it")]
    NotUpstream(CopyPath),
    #[error("upstream has no {0} to leave out here, so what the copy keeps has to change with it")]
    NothingToLeaveOut(CopyPath),
    #[error("the kept crates read files the copy doesn't keep, so add them to KEPT: {}", listed(.0))]
    ReadsUnkept(Vec<Read>),
    #[error("cutting the members out of upstream's Cargo.toml would change more of it, so cut it by hand")]
    MembersEntangled,
    #[error("upstream's workspace doesn't list {} as members, so the copy can't keep them as is", listed(.0))]
    NotMembers(Vec<CopyPath>),
    #[error("this repository's ignore files would keep these out of git, so make exceptions: {}", listed(.0))]
    Ignored(Vec<CopyPath>),
    #[error("{COPY} has files but no {RECORD} to say what they were made from: {}", listed(.0))]
    Unrecorded(Vec<CopyPath>),
    #[error("there's no copy, or no {RECORD} to say what it was made from")]
    NoCopy,
    #[error("git couldn't merge upstream's changes with ours: {0}")]
    Unmerged(String),
    #[error("the copy's manifests don't say {0}")]
    NoVersion(&'static str),
    #[error("Cargo.toml has no one `{0} = {{ … version = \"…\" }}` line to move")]
    NoVersionLine(&'static str),
    #[error("`git {command}` failed: {stderr}")]
    Git { command: String, stderr: String },
    #[error("git couldn't be run: {0}")]
    NoGit(#[source] io::Error),
    #[error("{}: {source}", path.display())]
    Toml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Says which path an I/O error came from.
pub(crate) trait At<T> {
    fn at(self, path: &Path) -> Result<T, Error>;
}

impl<T> At<T> for io::Result<T> {
    fn at(self, path: &Path) -> Result<T, Error> {
        self.map_err(|source| Error::Io { path: path.to_path_buf(), source })
    }
}

fn listed(items: &[impl Display]) -> String {
    items.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
}
