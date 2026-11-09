//! Starting a hub when there's none, which a client does so its user never has to: which daemon it runs, and running
//! it apart from this client. A stamped build's hubs run from its own copy of the daemon, for the reason "Updating
//! the app" in `docs/architecture.md` gives, and any other build starts the daemon where it is.

use std::fs::{File, OpenOptions, Permissions};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use protocol::StampedBuild;
use protocol::paths::{DAEMON_NAME, Paths};

use super::{Client, Result, hub_is_missing};

/// Far longer than a forked child takes to exec, which is all a busy copy waits for. See [`waiting_out_busy`].
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

impl Client {
    /// Connects, starting a hub first if none is running. The hub is a separate process that
    /// outlives this client, which is why closing a client doesn't stop its agents.
    pub fn connect_or_start(paths: &Paths, daemon: &Daemon, client_name: &str) -> Result<Client> {
        match Client::connect(&paths.socket, client_name) {
            Ok(client) => return Ok(client),
            Err(e) if hub_is_missing(&e) => {}
            Err(e) => return Err(e),
        }
        start_hub(paths, daemon)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match Client::connect(&paths.socket, client_name) {
                Ok(client) => return Ok(client),
                Err(e) if hub_is_missing(&e) && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// The daemon a client starts a hub from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Daemon {
    pub path: PathBuf,
    /// Which build it is, if it's stamped. Only the daemon beside a client is known to be the client's build, so one
    /// named any other way has none.
    pub build: Option<StampedBuild>,
}

impl Daemon {
    /// The one `named` by `ROUNDHOUSE_DAEMON_BIN`, started where it is, or else the one that ships next to this
    /// binary.
    pub fn find(named: Option<PathBuf>) -> io::Result<Self> {
        match named {
            Some(path) => Ok(Self::in_place(path)),
            None => Self::beside(&std::env::current_exe()?),
        }
    }

    /// One started where it is.
    pub fn in_place(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), build: None }
    }

    /// The one that ships next to `exe`, which is this build. Follows symlinks first, so a hub started through an
    /// install's `current` link stays on that version, whichever one the link names later.
    fn beside(exe: &Path) -> io::Result<Self> {
        Ok(Self { path: exe.canonicalize()?.with_file_name(DAEMON_NAME), build: protocol::STAMPED_BUILD })
    }

    /// The file to start a hub from: the build's own copy, made the first time it's needed, or else the daemon itself.
    fn to_start(&self, paths: &Paths) -> io::Result<PathBuf> {
        let Some(build) = &self.build else {
            return Ok(self.path.clone());
        };
        let copy = paths.hub_copy(build);
        // A copy only appears once it's whole and checked, and nothing replaces it, so one that's there is ready.
        if !copy.exists() {
            publish_copy(&self.path, build, &copy).map_err(|e| {
                io::Error::new(e.kind(), format!("copying {} to {}: {e}", self.path.display(), copy.display()))
            })?;
        }
        Ok(copy)
    }
}

fn start_hub(paths: &Paths, daemon: &Daemon) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    use std::os::unix::process::CommandExt;
    // Only sets the mode if it creates the directory, because ROUNDHOUSE_HOME can point anywhere.
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&paths.data_dir)?;
    let log = OpenOptions::new().create(true).append(true).mode(0o600).open(&paths.log)?;
    let exe = daemon.to_start(paths)?;
    let mut command = Command::new(&exe);
    command
        .args(["hub", "--on-demand"])
        // The hub this client will look for, whatever this process's environment says.
        .env(protocol::paths::ENV_HOME, &paths.data_dir)
        .env(protocol::paths::ENV_WORKSPACES, &paths.workspaces_root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    // SAFETY: setsid is async-signal-safe, which is all pre_exec requires. It detaches the hub from
    // this terminal and process group, so closing the client doesn't reach it.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    waiting_out_busy(|| command.spawn())
        .map_err(|e| io::Error::new(e.kind(), format!("starting {}: {e}", exe.display())))?;
    Ok(())
}

/// Runs a freshly written copy, waiting out its being busy. Linux won't run a file while anything has it open for
/// writing, and a fork on another thread while the copy was being written leaves its child holding it until it execs.
fn waiting_out_busy<T>(mut run: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let deadline = Instant::now() + BUSY_TIMEOUT;
    loop {
        match run() {
            Err(e) if e.kind() == io::ErrorKind::ExecutableFileBusy && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            ran => return ran,
        }
    }
}

/// Writes the copy under a temporary name beside it, checks that it's `build`, and only then links it in as `copy`,
/// so a half-written or wrong copy never runs. A link fails rather than replace a copy that another client published
/// first, which a hub may have just started from.
fn publish_copy(original: &Path, build: &StampedBuild, copy: &Path) -> io::Result<()> {
    if let Some(dir) = copy.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let (temporary, file) = create_temporary(copy)?;
    let published = write_executable(original, file).and_then(|()| check_build(&temporary, build)).and_then(|()| {
        match std::fs::hard_link(&temporary, copy) {
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            linked => linked,
        }
    });
    let _ = std::fs::remove_file(&temporary);
    published
}

/// Gives each copy in progress a temporary name of its own, so clients copying at once can't write into each other's.
static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Never opens a file that's there already. One with the same name was left by a client that was killed mid-copy, and
/// whose process ID this one now has.
fn create_temporary(copy: &Path) -> io::Result<(PathBuf, File)> {
    loop {
        let n = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let temporary = copy.with_file_name(format!(".{DAEMON_NAME}.{}.{n}", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&temporary) {
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            opened => return opened.map(|file| (temporary, file)),
        }
    }
}

/// Copies only the contents, so a quarantine mark on the original doesn't follow. A Mach-O binary carries its
/// signature inside. Closes the file, which Linux needs before it will run it.
fn write_executable(original: &Path, mut file: File) -> io::Result<()> {
    io::copy(&mut File::open(original)?, &mut file)?;
    file.set_permissions(Permissions::from_mode(0o755))?;
    // Before it's linked in, so a crash can't leave a copy in place that's missing part of what was written.
    file.sync_all()
}

/// The daemon beside a client can be another build: one still running after an update replaced its install directory
/// would find the new build's daemon there.
fn check_build(file: &Path, build: &StampedBuild) -> io::Result<()> {
    let output = waiting_out_busy(|| Command::new(file).arg("--version").stderr(Stdio::null()).output())?;
    let expected = format!("{DAEMON_NAME} {}", build.version());
    let reported = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && reported.trim_end() == expected {
        return Ok(());
    }
    Err(io::Error::other(format!("it's {:?} rather than this build, {expected:?}", reported.trim_end())))
}

#[cfg(test)]
mod tests;
