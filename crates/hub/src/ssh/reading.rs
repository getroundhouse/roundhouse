//! Running `ssh -T git@github.com`, and reading whose key GitHub's answer names.

use std::process::Stdio;
use std::time::Duration;

use protocol::model::SshLogin;
use tokio::process::Command;

use crate::machine_environment::MachineEnvironment;

/// How long the hub lets `ssh` take, connecting and all, before it stops it, so one that hangs holds up only the read.
const READ_WITHIN: Duration = Duration::from_secs(15);

/// What `ssh` is asked: to connect to github.com as `git`, which GitHub answers by naming whose key it took, and
/// closing. It never asks anything, so an unknown host key or a key with a passphrase fails, rather than waiting on a
/// prompt nobody sees, and it adds nothing to `known_hosts`. A connection that takes long gives up before
/// [`READ_WITHIN`] does.
const ASKED: [&str; 7] = ["-T", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "--", "git@github.com"];

/// `ssh` on the hub's machine, run in the machine's environment, so it's found on the profile's `PATH`, and reaches
/// the session's SSH agent.
pub(crate) struct Ssh {
    machine: MachineEnvironment,
}

impl Ssh {
    pub(crate) fn new(machine: MachineEnvironment) -> Self {
        Self { machine }
    }

    /// Whose key GitHub takes from this machine, or that it takes none, or [`SshLogin::CouldntTell`] when there's no
    /// `ssh`, it took longer than [`READ_WITHIN`], or it printed anything else, as when github.com's host key isn't one
    /// it knows, or it couldn't connect.
    pub(crate) async fn read(&self) -> SshLogin {
        let mut command = Command::new("ssh");
        command.args(ASKED).stdin(Stdio::null()).kill_on_drop(true);
        self.machine.given(&[]).await.apply(command.as_std_mut());

        match tokio::time::timeout(READ_WITHIN, command.output()).await {
            Ok(Ok(output)) => said(&String::from_utf8_lossy(&output.stderr)),
            Ok(Err(_)) | Err(_) => SshLogin::CouldntTell,
        }
    }
}

/// What GitHub's answer on stderr says: `Hi ada! You've successfully authenticated, …` for a key it takes as `ada`'s,
/// and `… Permission denied (publickey).` for none. A deploy key's greeting names a repository, `owner/name`, rather
/// than a user, so it's no user's.
pub(super) fn said(stderr: &str) -> SshLogin {
    for line in stderr.lines() {
        if let Some((username, _)) = line.strip_prefix("Hi ").and_then(|rest| rest.split_once('!'))
            && is_username(username)
        {
            return SshLogin::Key { username: username.into() };
        }
    }
    if stderr.contains("Permission denied (publickey)") { SshLogin::NoKey } else { SshLogin::CouldntTell }
}

/// Whether `name` could be a GitHub username: letters, digits and hyphens, as GitHub allows.
fn is_username(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}
