//! Placing a build's programs beside the builds placed before it, and pointing the service's link at it.

use std::ffi::OsString;
use std::path::Path;

use anyhow::Context;
use protocol::StampedBuild;
use protocol::paths::Paths;

/// Puts this build's daemon and command line, from beside this program, where the service runs this build from, and
/// points the service at it.
pub(crate) fn place(paths: &Paths) -> anyhow::Result<()> {
    let build = crate::BUILD.stamped().context("only a build stamped with its version can be installed")?;
    let this = std::env::current_exe().context("finding this program")?;
    let from = this.parent().context("finding where this program is")?;
    let name = this.file_name().context("naming this program")?;
    place_build(paths, &build, from, &[OsString::from(protocol::paths::daemon_file()), name.to_owned()])
}

/// Copies each of `programs` from `from` into `build`'s directory for the service, makes the hub's log, and points the
/// service's link at the build. Each file goes in under another name first and is renamed into place, so a hub or a
/// worker running from one already there goes on with the file it started from. The log is made here so it's the
/// user's, rather than the service manager's, which opens it for the hub.
pub(super) fn place_build(
    paths: &Paths,
    build: &StampedBuild,
    from: &Path,
    programs: &[OsString],
) -> anyhow::Result<()> {
    let to = paths.service_build(build);
    private_files::create_dir_all(&to).with_context(|| format!("making {}", to.display()))?;
    for program in programs {
        copy_into(&from.join(program), &to.join(program))?;
    }
    private_files::file_options()
        .create(true)
        .append(true)
        .open(&paths.log)
        .with_context(|| format!("making {}", paths.log.display()))?;
    let current = paths.service_current();
    let linking = current.with_file_name(format!(".current.{}", std::process::id()));
    let _ = std::fs::remove_file(&linking);
    link(Path::new(&build.dir_name()), &linking).with_context(|| format!("linking {}", linking.display()))?;
    std::fs::rename(&linking, &current).with_context(|| format!("pointing {} at {}", current.display(), to.display()))
}

/// Copies `from` to `to`, by way of a name beside it, so nothing ever runs a file that's half written.
fn copy_into(from: &Path, to: &Path) -> anyhow::Result<()> {
    let name = to.file_name().context("naming the copy")?.to_string_lossy();
    let partial = to.with_file_name(format!(".{name}.{}", std::process::id()));
    std::fs::copy(from, &partial).with_context(|| format!("copying {} to {}", from.display(), partial.display()))?;
    std::fs::rename(&partial, to).with_context(|| format!("moving {} to {}", partial.display(), to.display()))
}

#[cfg(unix)]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}

#[cfg(windows)]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, at)
}
