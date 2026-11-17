//! Building a program a test runs.

use std::process::Command;

/// How the names of what Cargo sets for a package's own crate start, as it compiles it and as it runs its tests.
const PACKAGES_OWN_PREFIXES: [&str; 3] = ["CARGO_PKG_", "CARGO_MANIFEST_", "CARGO_BIN_"];

/// The rest of what Cargo sets for a package's own crate, by name.
const PACKAGES_OWN: [&str; 4] = ["CARGO_CRATE_NAME", "CARGO_PRIMARY_PACKAGE", "CARGO_TARGET_TMPDIR", "OUT_DIR"];

/// `cargo build --workspace`, for a test to build a program it runs, as `docs/rust.md` says, without what Cargo set in
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
