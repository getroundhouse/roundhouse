//! The daemons these tests start: building them, and the environment they run in.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use process::testing::WholeEnvironment;

/// The binaries live in other packages, so `cargo test -p desktop` wouldn't otherwise rebuild them. They're in the
/// folder Cargo says it built the hub in, with the examples in its `examples`. Where it keeps this test binary, and so
/// what's near it, isn't something Cargo promises, and 1.100 moved it.
pub(crate) fn binaries() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        // The build says the folder here, for the tests a nextest run starts after it, which don't build.
        let said = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ui-binaries.dir");
        once_per_run("ui-binaries", || std::fs::write(&said, build_binaries()).unwrap());
        PathBuf::from(std::fs::read_to_string(&said).unwrap())
    })
}

/// Builds the hub and the examples, with the whole workspace's features as `docs/rust.md` says, and answers the folder
/// Cargo says it put the hub in. Naming the examples in the same build is what gives the hub those features too.
fn build_binaries() -> String {
    let output = process::testing::workspace_build()
        .args([
            "--bin",
            "roundhoused",
            "--example",
            "fake_claude",
            "--example",
            "stand_in_hub",
            "--message-format=json-render-diagnostics",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let hub = output
        .stdout
        .split(|&byte| byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .find(|message| message["reason"] == "compiler-artifact" && message["target"]["name"] == "roundhoused")
        .and_then(|message| message["executable"].as_str().map(PathBuf::from))
        .expect("Cargo says where it built the hub");
    hub.parent().unwrap().to_str().unwrap().to_string()
}

/// Builds once per nextest run, which starts a process for each test, so the `OnceLock` above alone would run cargo
/// for every test, and those take turns on its lock on the target directory. Under `cargo test`, once per process.
/// `name` names the lock, and the file that says which run built it.
fn once_per_run(name: &str, build: impl FnOnce()) {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    // Cargo makes it only as it compiles a test, and the build writes what it found there, nextest or not.
    std::fs::create_dir_all(dir).unwrap();
    let Ok(run) = std::env::var("NEXTEST_RUN_ID") else { return build() };
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

/// The whole environment of every daemon these tests start, whether a test starts it or the app does: what
/// [`WholeEnvironment::for_a_test`] gives, with a home of its own, so a real ~/.conductor/settings.toml or `gh` login
/// can't change what these tests do, a `gh` that's logged in to nothing, the fake agent, and the timers that only make
/// a test wait shortened. A hub a client starts is told where its data and workspaces are as it starts.
pub(crate) fn hub_environment() -> WholeEnvironment {
    // Under Cargo's target directory, so runs can't pile up in the system's. Conversation IDs are unique, so nothing
    // here carries over from one run to the next.
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let (timers, short) = hub::Timers::FOR_TESTS.env();
    let (worker_timers, worker_short) = worker::Timers::FOR_TESTS.env();
    let mut environment = WholeEnvironment::for_a_test(&scratch.join("ui-home"));
    environment
        // Ahead of the person's own, so what a hub hears of `gh` doesn't turn on whether they have one, or which. A
        // test that has `gh` answer puts its own ahead of this one.
        .put_first_on_path(logged_out_gh(scratch))
        .set("ROUNDHOUSE_CLAUDE_BIN", binaries().join("examples/fake_claude"))
        // So a test waits out only the time limits it's about. The hub passes the workers' on to them.
        .set(timers, short)
        .set(worker_timers, worker_short)
        .set("FAKE_CLAUDE_STATE", scratch.join("ui-agent-state"))
        // So an agent started again doesn't sit through the CLI's 2 s before it asks after a task.
        .set("FAKE_CLAUDE_POLL_DEFAULT_MS", worker::Timers::FOR_TESTS.wait_poll.as_millis().to_string());
    environment
}

/// `gh` as it is before `gh auth login`, which is what the person's own printed in a home of the tests' own: it names
/// no login for github.com, and refuses everything else.
const LOGGED_OUT_GH: &str = r#"#!/bin/sh
if [ "$1 $2" = 'auth status' ]; then
  echo 'You are not logged into any GitHub hosts. To log in, run: gh auth login' >&2
  exec echo '{"hosts":{}}'
fi
echo 'To get started with GitHub CLI, please run:  gh auth login' >&2
exit 4
"#;

/// The folder that holds [`LOGGED_OUT_GH`] as `gh`, under `scratch`. Every test process shares it, so it's only ever
/// replaced whole, by a rename, never written where a worker of another test may be running it. It's put there by
/// `install`, so this multithreaded process never has it open for writing, which a child another test forks would
/// inherit, and Linux then refuses to run it, as "text file busy".
fn logged_out_gh(scratch: &Path) -> &'static Path {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        let bin = scratch.join("ui-gh");
        let gh = bin.join("gh");
        if std::fs::read_to_string(&gh).is_ok_and(|there| there == LOGGED_OUT_GH) {
            return bin;
        }
        std::fs::create_dir_all(&bin).unwrap();
        // This process's own, so two processes writing it at once don't meet.
        let mine = |extension| bin.join(format!("gh-{}.{extension}", std::process::id()));
        let (script, installed) = (mine("sh"), mine("next"));
        std::fs::write(&script, LOGGED_OUT_GH).unwrap();
        let status = std::process::Command::new("install").args(["-m", "755"]).arg(&script).arg(&installed).status();
        assert!(status.is_ok_and(|status| status.success()), "install the stand-in for gh");
        std::fs::rename(&installed, &gh).unwrap();
        std::fs::remove_file(&script).unwrap();
        bin
    })
}
