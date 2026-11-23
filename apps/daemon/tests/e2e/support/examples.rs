//! The programs the tests run in place of others: the agent, and a provider's adapter.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::Stack;

/// What the hub runs as the `claude` CLI.
pub(super) fn fake_agent() -> &'static Path {
    static FAKE: OnceLock<PathBuf> = OnceLock::new();
    FAKE.get_or_init(|| example("fake_claude"))
}

/// The daemon every stack's hub and workers are, which takes what a test gives it.
pub(crate) fn daemon() -> &'static Path {
    static DAEMON: OnceLock<PathBuf> = OnceLock::new();
    DAEMON.get_or_init(|| example("daemon_for_tests"))
}

/// What [`Stack::make_through_an_adapter`] runs.
fn recording_adapter() -> &'static Path {
    static ADAPTER: OnceLock<PathBuf> = OnceLock::new();
    ADAPTER.get_or_init(|| example("recording_adapter"))
}

/// `cargo test --test e2e` doesn't build examples, so a plain path could run an out-of-date one without saying so.
/// Building here means tests always use the current one. With the whole workspace's features, as `docs/standards/testing.md` says.
fn example(name: &str) -> PathBuf {
    let bin_dir = Path::new(env!("CARGO_BIN_EXE_roundhoused")).parent().unwrap();
    let mut build = process::testing::workspace_build();
    build.args(["--example", name]);
    if bin_dir.file_name().is_some_and(|name| name == "release") {
        build.arg("--release");
    }
    process::testing::once_per_run(Path::new(env!("CARGO_TARGET_TMPDIR")), &format!("e2e-{name}"), || {
        let output = build.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    });
    bin_dir.join("examples").join(name)
}

impl Stack {
    /// Makes workspaces from now on through the local provider behind an adapter, as through another provider's, so
    /// [`Stack::instances_made`] and [`Stack::tickets_given`] say what the adapter was asked. Starts the hub again, since it reads its
    /// providers as it starts.
    pub(crate) fn make_through_an_adapter(&mut self) {
        self.write_adapter("home/clones", &[], &serde_json::json!([]));
    }

    /// As [`Stack::make_through_an_adapter`], with the providers file naming `workers`, the builds of the worker for
    /// other machines, which [`Stack::workers_started`] says the adapter was given.
    pub(crate) fn make_through_an_adapter_given(&mut self, workers: &serde_json::Value) {
        self.write_adapter("home/clones", &[], workers);
    }

    /// Makes workspaces from now on through an adapter declaring [`WorkerFilesystem::Separate`], keeping its clones
    /// in a directory apart from the hub's own `home/clones` so tests prove it never reads the hub's disk.
    pub(crate) fn make_through_separate_adapter(&mut self) {
        self.write_adapter("adapter-clones", &["--separate"], &serde_json::json!([]));
    }

    /// Makes workspaces from now on through an adapter declaring [`WorkerFilesystem::Separate`] and
    /// [`ClonePersistence::Ephemeral`], which wipes `adapter-clones` whenever an instance is destroyed.
    pub(crate) fn make_through_ephemeral_adapter(&mut self) {
        self.write_adapter("adapter-clones", &["--separate", "--ephemeral"], &serde_json::json!([]));
    }

    fn write_adapter(&mut self, clones: &str, extra: &[&str], workers: &serde_json::Value) {
        self.kill_hub();
        let path = |name: &str| self.path(name).to_str().unwrap().to_string();
        let mut args = vec![
            "--worker".into(),
            daemon().to_str().unwrap().into(),
            "--instances".into(),
            path("adapter-instances"),
            "--workspaces-root".into(),
            path("workspaces"),
            "--clones".into(),
            path(clones),
            "--made".into(),
            path("instances-made"),
            "--tickets".into(),
            path("tickets-given"),
        ];
        args.extend(extra.iter().map(|flag| (*flag).to_string()));
        let adapter = serde_json::json!({"name": "recording", "program": recording_adapter(), "args": args});
        let file = serde_json::json!({"default": "recording", "providers": [adapter], "workers": workers});
        std::fs::write(self.path("home/providers.json"), file.to_string()).unwrap();
        self.start_hub();
    }

    /// The ID of each instance the adapter made, in turn, once [`Stack::make_through_an_adapter`] has been called.
    pub(crate) fn instances_made(&self) -> Vec<String> {
        self.lines_of("instances-made")
    }

    /// The ID of each instance the adapter started a worker on, in turn, with the ticket it gave the worker, as for
    /// [`Stack::instances_made`].
    pub(crate) fn tickets_given(&self) -> Vec<(String, String)> {
        let given = self.lines_of("tickets-given");
        given.iter().map(|line| line.split_once(' ').unwrap()).map(|(id, t)| (id.into(), t.into())).collect()
    }

    /// The workers the adapter was given at each start, in turn, once [`Stack::make_through_an_adapter_given`] has been
    /// called.
    pub(crate) fn workers_started(&self) -> Vec<Vec<protocol::provider::WorkerBinary>> {
        self.lines_of("instances-made.started").iter().map(|line| serde_json::from_str(line).unwrap()).collect()
    }

    fn lines_of(&self, name: &str) -> Vec<String> {
        let written = std::fs::read_to_string(self.path(name)).unwrap_or_default();
        written.lines().map(str::to_string).collect()
    }
}
