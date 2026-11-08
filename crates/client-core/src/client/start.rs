//! Starting a hub when there's none, which a client does so its user never has to.

use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use protocol::paths::Paths;

use super::{Client, Result, hub_is_missing};

impl Client {
    /// Connects, starting a hub first if none is running. The hub is a separate process that
    /// outlives this client, which is why closing a client doesn't stop its agents.
    pub fn connect_or_start(paths: &Paths, daemon: &Path, client_name: &str) -> Result<Client> {
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

/// The daemon that ships next to `exe`. Follows symlinks first, so a hub started through an install's
/// `current` link stays on that version, whichever one the link names later.
pub fn daemon_beside(exe: &Path) -> std::io::Result<PathBuf> {
    Ok(exe.canonicalize()?.with_file_name("roundhoused"))
}

fn start_hub(paths: &Paths, daemon: &Path) -> Result<()> {
    // Only sets the mode if it creates the directory, because ROUNDHOUSE_HOME can point anywhere.
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&paths.data_dir)?;
    let log = std::fs::OpenOptions::new().create(true).append(true).mode(0o600).open(&paths.log)?;
    let mut command = Command::new(daemon);
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
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.spawn().map_err(|e| std::io::Error::new(e.kind(), format!("starting {}: {e}", daemon.display())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_daemon_beside_a_linked_binary_is_in_the_version_the_link_named() {
        let dir = tempfile::tempdir().unwrap();
        let version = dir.path().canonicalize().unwrap().join("abc123");
        std::fs::create_dir(&version).unwrap();
        std::fs::write(version.join("roundhouse"), "").unwrap();
        std::os::unix::fs::symlink(&version, dir.path().join("current")).unwrap();

        let daemon = daemon_beside(&dir.path().join("current/roundhouse")).unwrap();

        assert_eq!(daemon, version.join("roundhoused"));
    }
}
