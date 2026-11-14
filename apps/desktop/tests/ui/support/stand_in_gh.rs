//! A real hub with a stand-in for `gh` at the front of its `PATH`, which reports the pull request a test has it
//! report, as the hub's own tests do in `apps/daemon/tests/e2e/pull_requests.rs`.

#![cfg(test)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use client_core::Client;

use super::{Stack, TIMEOUT, Ui, binaries};

/// What `gh pr view` prints for a pull request in this state, such as `OPEN` or `MERGED`, for [`StandInGh::report`].
pub(crate) fn pull_request(state: &str) -> String {
    format!(r#"{{"number":7,"url":"https://example.com/pull/7","title":"T","state":"{state}","isDraft":false}}"#)
}

/// The hub, and the file its `gh` prints. Stops the hub with the test, even a failed one.
pub(crate) struct StandInGh {
    hub: Child,
    socket: PathBuf,
    answer: PathBuf,
    remote: PathBuf,
}

impl StandInGh {
    /// Has `gh pr view` print this from now on, as GitHub would. Until a test calls this, it says there's no pull
    /// request, as it does for a branch with none.
    pub(crate) fn report(&self, json: &str) {
        write_whole(&self.answer, json);
    }

    /// A complete GraphQL response for explicit PR reads, apart from the branch's `pr view` answer.
    pub(crate) fn report_remote(&self, json: &str) {
        write_whole(&self.remote, json);
    }

    /// Has `gh` fail from now on, as it does when it can't reach GitHub, which says nothing about the pull request.
    pub(crate) fn go_offline(&self) {
        std::fs::write(self.answer.with_extension("offline"), "").unwrap();
    }
}

impl Drop for StandInGh {
    /// The way the app stops a hub, so it has removed its socket by the time the stack looks.
    fn drop(&mut self) {
        if let Ok(client) = Client::connect(&self.socket, "test") {
            let _ = client.hub_shutdown();
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while matches!(self.hub.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.hub.kill();
        let _ = self.hub.wait();
    }
}

impl Stack {
    /// Starts the hub before the app opens, which then uses it rather than starting its own.
    pub(crate) fn start_hub_with_stand_in_gh(&self) -> StandInGh {
        let bin = self.dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let answer = self.dir.path().join("gh-pr-view.json");
        let remote = self.dir.path().join("gh-remote.json");
        // It's in no merge queue. Commands other than PR reads fail, such as asking how much of GitHub's limit is left, which
        // then goes unknown. A pull request that doesn't say its newest commit gets the one checked out, as one
        // pushed from the worktree has, so it's the branch's own.
        let (offline, answer_path) = (answer.with_extension("offline"), answer.display());
        let script = format!(
            r#"#!/bin/sh
[ "$1 $2" = 'api graphql' ] && exec echo '{{"data":{{"resource":{{"mergeQueueEntry":null}}}}}}'
[ "$1 $2" = 'repo view' ] && exec echo '{{"url":"https://github.com/example/project","nameWithOwner":"example/project"}}'
[ -f '{offline}' ] && {{ echo 'error connecting to api.github.com' >&2; exit 1; }}
[ "$1 $2" = 'api --hostname' ] && exec cat '{remote}'
[ "$1 $2" = 'pr view' ] || exit 1
[ -f '{answer_path}' ] && grep -q headRefOid '{answer_path}' && exec cat '{answer_path}'
[ -f '{answer_path}' ] && exec sed "1s/^{{/{{\"headRefOid\":\"$(git rev-parse HEAD)\",/" '{answer_path}'
echo 'no pull requests found for branch "x"' >&2
exit 1
"#,
            offline = offline.display(),
            remote = remote.display()
        );
        let gh = bin.join("gh");
        std::fs::write(&gh, script).unwrap();
        std::fs::set_permissions(&gh, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
        let hub = Command::new(binaries().join("roundhoused"))
            .args(["hub", "--on-demand"])
            .env(protocol::paths::ENV_HOME, &self.hub.paths.data_dir)
            .env(protocol::paths::ENV_WORKSPACES, &self.hub.paths.workspaces_root)
            // No login shell, so workers get this PATH, with the stand-in at its front.
            .env("PATH", path)
            .env("SHELL", "/usr/bin/false")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let socket = self.hub.paths.socket.clone();
        let deadline = Instant::now() + TIMEOUT;
        while Client::connect(&socket, "test").is_err() {
            assert!(Instant::now() < deadline, "timed out waiting for the hub to listen");
            std::thread::sleep(Duration::from_millis(10));
        }
        StandInGh { hub, socket, answer, remote }
    }
}

/// Whole or not at all, since the worker may run it at any moment.
fn write_whole(path: &Path, text: &str) {
    let next = path.with_extension("next");
    std::fs::write(&next, text).unwrap();
    std::fs::rename(&next, path).unwrap();
}

impl Ui<'_> {
    /// Lets the time pass after which the app reads the pull request showing again, and lets it read it.
    pub(crate) fn wait_out_a_pull_request_look(&mut self) {
        self.cx.executor().advance_clock(desktop::pull_request::READ_AGAIN);
        self.frame();
    }
}
