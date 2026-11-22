//! Building the app's Rust binary for an iPhone, and which of the workspace's crates it's built from.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, bail};

use crate::program;

const TARGET: &str = "aarch64-apple-ios";
/// The oldest iOS the app supports, which `apps/mobile-ios/ios/project.yml` sets for Xcode too.
const DEPLOYMENT_TARGET: &str = "17.0";

#[derive(Clone, Copy)]
pub(crate) enum Profile {
    Release,
    Debug,
}

impl Profile {
    /// Xcode's name for the configuration that packages a build in this profile.
    pub(crate) fn configuration(self) -> &'static str {
        match self {
            Self::Release => "Release",
            Self::Debug => "Debug",
        }
    }
}

/// Builds `roundhouse-ios` for any iPhone, and returns where Cargo put it.
pub(crate) fn compile(root: &Path, profile: Profile) -> anyhow::Result<PathBuf> {
    let mut build = cargo(root);
    build
        .args(["build", "--locked", "--target", TARGET, "-p", "mobile-ios", "--bin", "roundhouse-ios"])
        .env("IPHONEOS_DEPLOYMENT_TARGET", DEPLOYMENT_TARGET);
    let folder = match profile {
        Profile::Release => {
            build.arg("--release");
            "release"
        }
        Profile::Debug => "debug",
    };
    program::run(&mut build)?;
    Ok(target_dir(root)?.join(TARGET).join(folder).join("roundhouse-ios"))
}

/// Builds `roundhouse-ios` in release, as an upload packages it, and copies it to `out`.
pub(crate) fn compile_to(root: &Path, out: &Path) -> anyhow::Result<()> {
    let built = compile(root, Profile::Release)?;
    fs::copy(&built, out).with_context(|| format!("couldn't copy {} to {}", built.display(), out.display()))?;
    Ok(())
}

/// The folders of the workspace's own crates and copies the app builds for an iPhone, as paths from `root`. A crate
/// it stopped building changed a `Cargo.toml` or `Cargo.lock`, so whoever compares these compares those too.
pub(crate) fn app_crates(root: &Path) -> anyhow::Result<Vec<String>> {
    let tree = program::output(cargo(root).args([
        "tree",
        "--locked",
        "-p",
        "mobile-ios",
        "--target",
        TARGET,
        "-e",
        "normal,build",
        "--prefix",
        "none",
        "-f",
        "{p}",
    ]))?;
    let crates = local_crates(&tree, root);
    if crates.is_empty() {
        bail!("`cargo tree` listed none of the app's crates");
    }
    Ok(crates)
}

/// The folders in `cargo tree`'s lines, such as `look v0.1.0 (/repo/crates/look)`, from `root`, each once. A crate
/// from a registry has none.
fn local_crates(tree: &str, root: &Path) -> Vec<String> {
    let mut crates: Vec<String> = tree
        .lines()
        .filter_map(|line| {
            let folder = line.split_once(" (")?.1.split_once(')')?.0;
            Some(Path::new(folder).strip_prefix(root).ok()?.to_string_lossy().into_owned())
        })
        .collect();
    crates.sort();
    crates.dedup();
    crates
}

fn target_dir(root: &Path) -> anyhow::Result<PathBuf> {
    let metadata = program::output(cargo(root).args(["metadata", "--format-version", "1", "--no-deps"]))?;
    let metadata: serde_json::Value =
        serde_json::from_str(&metadata).context("`cargo metadata` printed what isn't JSON")?;
    let Some(dir) = metadata["target_directory"].as_str() else { bail!("`cargo metadata` named no target directory") };
    Ok(PathBuf::from(dir))
}

fn cargo(root: &Path) -> Command {
    let mut command = Command::new("cargo");
    command.current_dir(root);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_apps_crates_are_the_folders_in_the_workspace_each_once() {
        let tree = "mobile-ios v0.1.0 (/repo/apps/mobile-ios)\n\
                    anyhow v1.0.104\n\
                    look v0.1.0 (/repo/crates/look)\n\
                    look v0.1.0 (/repo/crates/look) (*)\n\
                    gpui-kit v0.3.0 (/repo/third_party/gpui-kit/vendor/crates/kit)\n\
                    elsewhere v0.1.0 (/other/elsewhere)\n";

        let crates = local_crates(tree, Path::new("/repo"));

        assert_eq!(crates, ["apps/mobile-ios", "crates/look", "third_party/gpui-kit/vendor/crates/kit"]);
    }
}
