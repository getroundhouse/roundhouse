//! Fetches libvpx's source at the commit `third_party/libvpx/COMMIT` pins, with git, as libghostty-vt's build script
//! fetches Ghostty's. Git names every file by its hash, so what's fetched is what `tools/libvpx-sync.sh` wrote the
//! configs from, byte for byte.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

const REPOSITORY: &str = "https://github.com/webmproject/libvpx.git";

/// libvpx's source at `commit`, in `out`, which an earlier build fetched, or this one does.
pub(crate) fn source(commit: &str, out: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let source = out.join(format!("libvpx-{commit}"));
    if source.exists() {
        return Ok(source);
    }
    // Only a whole fetch is renamed to `source`, so one that was stopped starts again.
    let partial = out.join("libvpx-partial");
    if partial.exists() {
        std::fs::remove_dir_all(&partial)?;
    }
    std::fs::create_dir_all(&partial)?;
    let local = local_variables(out)?;
    git(&partial, &local, &["-c", "init.defaultBranch=main", "init", "-q"])?;
    git(&partial, &local, &["fetch", "-q", "--depth", "1", REPOSITORY, commit])?;
    // As the commit has them, though Git for Windows writes a text file's line endings as Windows' by default.
    let checkout = ["-c", "core.autocrlf=false", "-c", "advice.detachedHead=false", "checkout", "-q", "FETCH_HEAD"];
    git(&partial, &local, &checkout)?;
    // An older commit's, which would otherwise stay as long as `OUT_DIR` does.
    for entry in std::fs::read_dir(out)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with("libvpx-") && entry.path() != partial {
            std::fs::remove_dir_all(entry.path())?;
        }
    }
    std::fs::rename(&partial, &source)?;
    Ok(source)
}

/// What git reads from its environment about the repository it's in, by its own list. A build run from a git hook,
/// such as one that runs the tests before a push, has `GIT_DIR` and `GIT_INDEX_FILE` set to the checkout's, which
/// would have the fetch write there. Not the settings git was given, such as a proxy, which git itself hands on to a
/// command in another repository.
fn local_variables(dir: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let listed = Command::new("git")
        .current_dir(dir)
        .args(["rev-parse", "--local-env-vars"])
        .output()
        .map_err(|error| format!("couldn't run git, which fetches libvpx's source: {error}"))?;
    if !listed.status.success() {
        return Err(format!("git couldn't list its repository's variables: {}", listed.status).into());
    }
    let settings = ["GIT_CONFIG_PARAMETERS", "GIT_CONFIG_COUNT"];
    Ok(String::from_utf8(listed.stdout)?.lines().filter(|name| !settings.contains(name)).map(str::to_string).collect())
}

/// Runs git in `dir`, without any of `local`.
fn git(dir: &Path, local: &[String], arguments: &[&str]) -> Result<(), Box<dyn Error>> {
    let mut command = Command::new("git");
    command.current_dir(dir).args(arguments);
    for name in local {
        command.env_remove(name);
    }
    let status = command.status()?;
    if !status.success() {
        return Err(format!("couldn't fetch libvpx's source, with git {}: {status}", arguments.join(" ")).into());
    }
    Ok(())
}
