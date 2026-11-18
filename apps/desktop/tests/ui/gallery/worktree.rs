//! The worktree every screen's All files lists: a made-up repository's files, the same branch the diff screens review,
//! what's in the folders git ignores, and the text of the one opened.

#![cfg(test)]

use protocol::attachment::{AttachmentBytes, MediaType};
use protocol::model::{FileContents, WorktreeFiles};

use super::pictures;
use client_core::fake_hub::World;

/// Gives the world's hub the worktree to list, to read a folder of, and to open a file from.
pub(crate) fn put_in(world: &mut World) {
    let strings = |names: &[&str]| names.iter().map(ToString::to_string).collect::<Vec<_>>();
    world.worktree = WorktreeFiles { paths: strings(FILES), folders: strings(FOLDERS), truncated: false };
    for (folder, paths, folders) in IN_FOLDERS {
        let files = WorktreeFiles { paths: strings(paths), folders: strings(folders), truncated: false };
        world.worktree_folders.insert((*folder).to_string(), files);
    }
    world.worktree_files.insert(OPENED.to_string(), FileContents::Text { text: MODEL.to_string() });
    world.worktree_files.insert(README.to_string(), FileContents::Text { text: README_TEXT.to_string() });
    world.worktree_files.insert(PICTURE.to_string(), png(pictures::badge(SHOT_HEIGHT, false)));
}

/// An image, as the worker reads one.
pub(crate) fn png(data: Vec<u8>) -> FileContents {
    FileContents::Image { media_type: MediaType::Png, data: AttachmentBytes::new(data) }
}

/// The image the screen of an image opens, which the gallery's own folder holds.
pub(crate) const PICTURE: &str = ".context/gallery/changes.png";

/// How tall the made-up screenshots of the sidebar are, short enough that the file tab draws them big enough to read.
pub(crate) const SHOT_HEIGHT: u32 = 280;

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

/// The Markdown file the screens of one rendered open, as All files lists it.
pub(crate) const README: &str = "README.md";

/// What it holds: a little of each kind of block, as a README has.
pub(crate) const README_TEXT: &str = r"# Tidewater

A small service that keeps a team's tide tables in step with the harbour's sensors, and warns before a berth runs dry.

It reads each sensor every minute, keeps a week of readings in SQLite, and serves them at `/tides` for the dashboard. See [the design notes](docs/design.md) for why it polls rather than subscribes.

## Getting started

1. Install Rust 1.90 or later.
2. Copy `.env.example` to `.env`, and set `HARBOUR_URL`.
3. Run it:

```sh
cargo run -- --port 8080
```

## Configuration

| Variable | What it sets | Default |
|---|---|---|
| `HARBOUR_URL` | Where the sensors are read from | none |
| `POLL_SECONDS` | How often each sensor is read | `60` |
| `KEEP_DAYS` | How long readings are kept | `7` |

> A berth is flagged as running dry when the forecast low is under **1.2 m** for more than ten minutes.

## Status

- [x] Read the harbour's sensors
- [x] Serve readings at `/tides`
- [ ] Send warnings to the team's channel
";
