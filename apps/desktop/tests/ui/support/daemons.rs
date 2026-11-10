//! The daemons these tests start: building them, and the environment they run in.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The binaries live in other packages, so `cargo test -p desktop` wouldn't otherwise rebuild them.
pub(crate) fn binaries() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let output = Command::new(env!("CARGO"))
            .args([
                "build",
                "-p",
                "daemon",
                "--bin",
                "roundhoused",
                "--example",
                "fake_claude",
                "--example",
                "stand_in_hub",
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        // target/debug/deps/ui-xxxx
        std::env::current_exe().unwrap().parent().unwrap().parent().unwrap().to_path_buf()
    })
}

/// Where `gh` finds a login other than in `HOME`.
const GH_LOGINS: [&str; 6] =
    ["GH_TOKEN", "GITHUB_TOKEN", "GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN", "GH_CONFIG_DIR", "XDG_CONFIG_HOME"];

/// What the daemons these tests start need to run the fake agent. Set once, and never changed, because
/// tests here run in parallel and nothing else should be reading the environment at that moment.
pub(crate) fn point_daemons_at_the_fake_agent() {
    static DONE: OnceLock<()> = OnceLock::new();
    DONE.get_or_init(|| {
        // Under Cargo's target directory, so runs can't pile up in the system's. Conversation IDs are
        // unique, so nothing here carries over from one run to the next.
        let scratch = Path::new(env!("CARGO_TARGET_TMPDIR"));
        // SAFETY: guarded by the OnceLock, and this runs before any daemon or thread here reads it.
        unsafe {
            std::env::set_var("ROUNDHOUSE_CLAUDE_BIN", binaries().join("examples/fake_claude"));
            std::env::set_var("FAKE_CLAUDE_STATE", scratch.join("ui-agent-state"));
            // So a real ~/.conductor/settings.toml can't change what these tests do.
            std::env::set_var("HOME", scratch.join("ui-home"));
            // So `gh` can't reach GitHub as whoever runs the tests.
            for name in GH_LOGINS {
                std::env::remove_var(name);
            }
        }
    });
}
