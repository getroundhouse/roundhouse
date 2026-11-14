//! The programs the tests run in place of others: the agent, and a provider's adapter.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use super::Stack;

/// What the hub runs as the `claude` CLI.
pub(super) fn fake_agent() -> &'static Path {
    static FAKE: OnceLock<PathBuf> = OnceLock::new();
    FAKE.get_or_init(|| example("fake_claude"))
}

/// What [`Stack::make_through_an_adapter`] runs.
fn recording_adapter() -> &'static Path {
    static ADAPTER: OnceLock<PathBuf> = OnceLock::new();
    ADAPTER.get_or_init(|| example("recording_adapter"))
}

/// `cargo test --test e2e` doesn't build examples, so a plain path could run an out-of-date one without saying so.
/// Building here means tests always use the current one. With the whole workspace's features, as `docs/rust.md` says.
fn example(name: &str) -> PathBuf {
    let bin_dir = Path::new(env!("CARGO_BIN_EXE_roundhoused")).parent().unwrap();
    let mut build = Command::new(env!("CARGO"));
    build.args(["build", "--workspace", "--example", name]);
    if bin_dir.file_name().is_some_and(|name| name == "release") {
        build.arg("--release");
    }
    once_per_run(&format!("e2e-{name}"), || {
        let output = build.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    });
    bin_dir.join("examples").join(name)
}

/// Builds once per nextest run, which starts a process for each test, so the `OnceLock`s above alone would run cargo
/// for every test, and those take turns on its lock on the target directory. Under `cargo test`, once per process.
/// `name` names the lock, and the file that says which run built it.
fn once_per_run(name: &str, build: impl FnOnce()) {
    let Ok(run) = std::env::var("NEXTEST_RUN_ID") else { return build() };
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    // Cargo makes it only as it compiles a test.
    std::fs::create_dir_all(dir).unwrap();
    let lock = File::create(dir.join(format!("{name}.lock"))).unwrap();
    // Let go as it drops, even in a panic.
    lock.lock().unwrap();
    let last_run = dir.join(format!("{name}.run"));
    if std::fs::read_to_string(&last_run).is_ok_and(|last| last == run) {
        return;
    }
    build();
    std::fs::write(last_run, run).unwrap();
}

impl Stack {
    /// Makes workspaces from now on through the local provider behind an adapter, as through another provider's, so
    /// [`Stack::instances_made`] says what each instance was called. Starts the hub again, since it reads its
    /// providers as it starts.
    pub(crate) fn make_through_an_adapter(&mut self) {
        self.kill_hub();
        let path = |name: &str| self.path(name).to_str().unwrap().to_string();
        let args = [
            "--worker".into(),
            env!("CARGO_BIN_EXE_roundhoused").into(),
            "--instances".into(),
            path("adapter-instances"),
            "--workspaces-root".into(),
            path("workspaces"),
            "--made".into(),
            path("instances-made"),
        ];
        let adapter = serde_json::json!({"name": "recording", "program": recording_adapter(), "args": args});
        let file = serde_json::json!({"default": "recording", "providers": [adapter]});
        std::fs::write(self.path("home/providers.json"), file.to_string()).unwrap();
        self.start_hub();
    }

    /// The name each instance was made with, in turn, once [`Stack::make_through_an_adapter`] has been called.
    pub(crate) fn instances_made(&self) -> Vec<String> {
        let made = std::fs::read_to_string(self.path("instances-made")).unwrap_or_default();
        made.lines().map(str::to_string).collect()
    }
}
