//! The worktree the All files screens browse: a made-up repository's files, the same branch the diff screens review,
//! what's in the folders git ignores, and the text of the one opened.

#![cfg(test)]

/// In order of path, as the worker lists them, ignored ones too, such as `.env`.
pub(crate) const FILES: &[&str] = &[
    ".env",
    ".github/workflows/ci.yml",
    ".gitignore",
    ".mailmap",
    "AGENTS.md",
    "Cargo.lock",
    "Cargo.toml",
    "README.md",
    "apps/desktop/Cargo.toml",
    "apps/desktop/src/app.rs",
    "apps/desktop/src/lib.rs",
    "apps/desktop/src/sidebar.rs",
    "apps/desktop/tests/ui/sidebar.rs",
    "crates/hub/Cargo.toml",
    "crates/hub/src/hub.rs",
    "crates/hub/src/hub/pull_requests.rs",
    "crates/hub/src/server.rs",
    "crates/protocol/Cargo.toml",
    "crates/protocol/src/client.rs",
    "crates/protocol/src/lib.rs",
    "crates/protocol/src/model.rs",
    "crates/worker/Cargo.toml",
    "crates/worker/src/git.rs",
    "crates/worker/src/lib.rs",
    "docs/spec/sidebar.md",
    "rustfmt.toml",
];

/// The folders git ignores whole, which are read when they're opened.
pub(crate) const FOLDERS: &[&str] = &[".context", "target"];

/// What's in each folder that's read when it's opened, as files and folders: `.context` holds only a folder, which
/// keeps a row of its own once it's read.
pub(crate) const IN_FOLDERS: &[(&str, &[&str], &[&str])] = &[
    (".context", &[], &[".context/gallery"]),
    (".context/gallery", &[".context/gallery/changes.png"], &[]),
    ("target", &["target/CACHEDIR.TAG"], &["target/debug"]),
];

/// The file the screen opens.
pub(crate) const OPENED: &str = "crates/protocol/src/model.rs";

/// What it holds.
pub(crate) const MODEL: &str = r#"//! Records the hub owns and serves.

use serde::{Deserialize, Serialize};

mod pull_request;
mod session;

pub use pull_request::{PrState, PullRequest};
pub use session::{AgentKind, Session, SessionStatus};

/// A repository added to a hub.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Repo {
    pub id: RepoId,
    pub name: String,
    /// Where it is on the hub's machine.
    pub path: String,
}

/// A name, a branch and a worktree in one repository, with its sessions and diff comments.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub repo_id: RepoId,
    pub name: String,
    pub branch: String,
    /// What the branch was made from, which diffs are taken against.
    pub base_ref: Option<String>,
    /// Where its worktree is, once it's been made.
    pub path: Option<String>,
    pub status: WorkspaceStatus,
    pub setup: SetupStatus,
    /// The first of ten ports set aside for this workspace, so workspaces don't collide.
    pub port_base: u16,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    Creating,
    Ready,
    Archived,
    #[serde(other)]
    Unknown,
}
"#;
