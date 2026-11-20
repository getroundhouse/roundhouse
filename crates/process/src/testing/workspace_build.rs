//! Building a program a test runs.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::Command;

/// How the names of what Cargo sets for a package's own crate start, as it compiles it and as it runs its tests.
const PACKAGES_OWN_PREFIXES: [&str; 3] = ["CARGO_PKG_", "CARGO_MANIFEST_", "CARGO_BIN_"];

/// The rest of what Cargo sets for a package's own crate, by name.
const PACKAGES_OWN: [&str; 4] = ["CARGO_CRATE_NAME", "CARGO_PRIMARY_PACKAGE", "CARGO_TARGET_TMPDIR", "OUT_DIR"];

/// `cargo build --workspace`, for a test to build a program it runs, as `docs/standards/testing.md` says, without what Cargo set in
/// the test's environment for the test's own package, such as `CARGO_MANIFEST_DIR`. The build that made the test
/// didn't have those, so to a build script that watches one, as ring's watches every variable it reads, it would have
/// changed, and the build would compile that crate and everything above it again in every run. Everything else, such
/// as `CARGO_HOME` or `CARGO_TARGET_DIR`, it keeps.
pub fn workspace_build() -> Command {
    let mut build = Command::new(env!("CARGO"));
    build.args(["build", "--workspace"]);
    for (name, _) in std::env::vars_os() {
        if name.to_str().is_some_and(is_set_for_the_package) {
            build.env_remove(name);
        }
    }
    build
}

/// Runs `build` once per nextest run, which starts a process for each test, so a `OnceLock` alone would run cargo for
/// every test, and those take turns on its lock on the target directory. Under `cargo test`, it's for the caller's
/// `OnceLock` to build once per process. `dir` is the test's `CARGO_TARGET_TMPDIR`, which this makes, since Cargo makes
/// it only as it compiles a test. `name` names the lock there, and the file that says which run built it.
///
/// # Panics
///
/// If `dir` can't be made, or the lock or the file that says the run can't be made or written there.
pub fn once_per_run(dir: &Path, name: &str, build: impl FnOnce()) {
    let failed = |doing: &str, e: std::io::Error| -> ! { panic!("couldn't {doing} in {}: {e}", dir.display()) };
    std::fs::create_dir_all(dir).unwrap_or_else(|e| failed("make it", e));
    let Ok(run) = std::env::var("NEXTEST_RUN_ID") else { return build() };
    let lock = File::create(dir.join(format!("{name}.lock"))).unwrap_or_else(|e| failed("make the lock", e));
    // Let go as it drops, even in a panic.
    lock.lock().unwrap_or_else(|e| failed("take the lock", e));
    let last_run = dir.join(format!("{name}.run"));
    if std::fs::read_to_string(&last_run).is_ok_and(|last| last == run) {
        return;
    }
    build();
    std::fs::write(last_run, run).unwrap_or_else(|e| failed("say which run built it", e));
}

/// Builds the hub and `examples` with [`workspace_build`], once per run as [`once_per_run`] says, and answers the folder
/// Cargo says it put the hub in, with the examples in its `examples`. Naming the examples in the same build is what
/// gives the hub the whole workspace's features, as `docs/standards/testing.md` says. Where Cargo keeps a test binary, and so what's
/// near it, isn't something it promises, and 1.100 moved it. `dir` and `name` are as [`once_per_run`] takes them.
///
/// # Panics
///
/// If the build fails or doesn't say where it put the hub, or if what it said can't be kept in `dir` and read back.
pub fn the_hub_and_examples(dir: &Path, name: &str, examples: &[&str]) -> PathBuf {
    let failed = |doing: &str, e: &dyn std::fmt::Display| -> ! { panic!("couldn't {doing}: {e}") };
    // The build says the folder here, for the tests a nextest run starts after it, which don't build.
    let said = dir.join(format!("{name}.dir"));
    once_per_run(dir, name, || {
        let mut build = workspace_build();
        build.args(["--bin", "roundhoused", "--message-format=json-render-diagnostics"]);
        for example in examples {
            build.args(["--example", example]);
        }
        let output = build.output().unwrap_or_else(|e| failed("run cargo", &e));
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let folder = (output.stdout.split(|&byte| byte == b'\n'))
            .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
            .find(|message| message["reason"] == "compiler-artifact" && message["target"]["name"] == "roundhoused")
            .and_then(|message| Some(PathBuf::from(message["executable"].as_str()?).parent()?.to_path_buf()))
            .unwrap_or_else(|| failed("find where Cargo built the hub", &"it didn't say"));
        std::fs::write(&said, folder.as_os_str().as_encoded_bytes()).unwrap_or_else(|e| failed("keep the folder", &e));
    });
    PathBuf::from(std::fs::read_to_string(&said).unwrap_or_else(|e| failed("read the folder", &e)))
}

fn is_set_for_the_package(name: &str) -> bool {
    PACKAGES_OWN_PREFIXES.iter().any(|prefix| name.starts_with(prefix)) || PACKAGES_OWN.contains(&name)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    /// `CARGO` is set for a test too, and isn't the package's own.
    #[test]
    fn a_workspace_build_leaves_out_what_cargo_set_for_the_tests_package_and_keeps_the_rest() {
        assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some(), "Cargo sets it for a test");
        assert!(std::env::var_os("CARGO").is_some(), "Cargo sets it for a test");

        let build = workspace_build();

        let left_out: Vec<&OsStr> =
            build.get_envs().filter(|(_, value)| value.is_none()).map(|(name, _)| name).collect();
        assert!(left_out.contains(&OsStr::new("CARGO_MANIFEST_DIR")), "{left_out:?}");
        assert!(!left_out.contains(&OsStr::new("CARGO")), "{left_out:?}");
        assert_eq!(build.get_envs().filter(|(_, value)| value.is_some()).count(), 0, "it sets nothing");
        assert_eq!(build.get_args().collect::<Vec<_>>(), ["build", "--workspace"]);
    }
}
