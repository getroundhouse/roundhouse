//! A script that holds the hub's work on a workspace open until the test lets it go, so the test can see what shows
//! meanwhile.

use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

use super::{Stack, commit_settings, git};

/// The script, and how often it has run. It's let go when this goes, so a test that fails leaves nothing waiting.
pub(crate) struct Held {
    released: PathBuf,
    runs: PathBuf,
}

impl Held {
    /// An archive script, committed, which every workspace made from now on runs.
    pub(crate) fn archive(stack: &Stack) -> Self {
        let held = Self::new(stack, "archive");
        commit_settings(stack, &format!("[scripts]\narchive = \"{}\"\n", held.script()));
        held
    }

    /// A hook git runs as it checks out a worktree, so it holds restoring a workspace, and making one.
    pub(crate) fn checkout(stack: &Stack) -> Self {
        let held = Self::new(stack, "checkout");
        let hooks = stack.repo().with_file_name("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join("post-checkout");
        std::fs::write(&hook, format!("#!/bin/sh\n{}\n", held.script())).unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        git(&stack.repo(), &["config", "core.hooksPath", hooks.to_str().unwrap()]);
        held
    }

    /// Its files are named for what it holds, so a test can hold more than one thing.
    fn new(stack: &Stack, what: &str) -> Self {
        let file = |name: &str| stack.repo().with_file_name(format!("{what}-{name}"));
        Self { released: file("released"), runs: file("runs") }
    }

    /// It gives up once the test's directory has gone too, which a failed test deletes as it lets the script go.
    fn script(&self) -> String {
        let (runs, released) = (self.runs.display(), self.released.display());
        let dir = self.released.parent().map_or_default(|dir| dir.display().to_string());
        format!("echo ran >> {runs}; until [ -f {released} ] || [ ! -d {dir} ]; do sleep 0.01; done")
    }

    pub(crate) fn release(&self) {
        std::fs::write(&self.released, "").unwrap();
    }

    pub(crate) fn runs(&self) -> usize {
        std::fs::read_to_string(&self.runs).map_or(0, |runs| runs.lines().count())
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        let _ = std::fs::write(&self.released, "");
    }
}
