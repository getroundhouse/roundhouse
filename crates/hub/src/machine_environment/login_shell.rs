//! Asking the user's login shell what their profile gives, started as a terminal app starts one: interactive, and as a
//! login shell.

use std::io;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use process::{ENVIRONMENT_PRINTER, Environment, PRINTS_ENVIRONMENT, printed_environment};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::time::Instant;

/// Variables, each a name and its value.
pub(super) type Vars = Vec<(String, String)>;

/// How long the login shell gets to answer, both reads together when there are two, since rc files can run anything,
/// and the hub waits for it before it serves.
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// When the reads starting now must have answered by.
pub(super) fn deadline() -> Instant {
    Instant::now() + LOGIN_SHELL_TIMEOUT
}

/// What marks the `PATH` the shell prints at each end, when it's asked for that alone, since rc files can print
/// anything around it.
pub(super) const PATH_MARKER: &str = "__ROUNDHOUSE_LOGIN_PATH__";

/// What the shell sets for itself, about where it is and how deep, rather than what the profile gives. `PWD` would
/// otherwise give everything the hub starts the folder the shell started in.
const THE_SHELLS_OWN: [&str; 4] = ["PWD", "OLDPWD", "SHLVL", "_"];

/// The user's login shell, which the hub asks once, as it starts.
pub(crate) struct LoginShell<'a> {
    pub(crate) shell: &'a str,
    /// Where it starts, so an rc file that acts on the folder it's in gives the same answer wherever the hub was started.
    pub(crate) home: Option<&'a Path>,
    /// What it's given beyond the hub's own environment, which it inherits: nothing in the product, and in a test, the
    /// test's whole environment in place of its own.
    pub(crate) given: &'a Environment,
}

/// Why the login shell's answer couldn't be read.
#[derive(Debug, thiserror::Error)]
pub(super) enum Unread {
    #[error("it couldn't be started: {0}")]
    Unstarted(io::Error),
    #[error("it didn't answer within {LOGIN_SHELL_TIMEOUT:?}")]
    TimedOut,
    /// It ended, or closed its output, without saying what it was asked.
    #[error("it ended without saying")]
    Unsaid,
}

impl LoginShell<'_> {
    /// Every variable it has once it has read the profile, but those it sets for itself. It runs `printer`, the daemon,
    /// which prints what it was started with, as [`process::print_environment`] says, so reading it rests on nothing of
    /// the machine's own, such as an `env` that takes `-0`.
    pub(super) async fn environment(&self, printer: &Path, until: Instant) -> Result<Vars, Unread> {
        let script = format!("\"${ENVIRONMENT_PRINTER}\" {PRINTS_ENVIRONMENT}");
        let given = self.given.clone().with(ENVIRONMENT_PRINTER, printer);
        let said = self.ask(&script, &given, until, |seen| printed_environment(seen).is_some()).await?;
        let vars = printed_environment(&said).ok_or(Unread::Unsaid)?;
        Ok(vars.into_iter().filter(|(name, _)| !THE_SHELLS_OWN.contains(&name.as_str())).collect())
    }

    /// Only its `PATH`, which `printenv` prints, as every shell can run it, fish too, where `$PATH` expands to a list.
    pub(super) async fn path(&self, until: Instant) -> Result<String, Unread> {
        let script = format!("printf '{PATH_MARKER}'; printenv PATH; printf '{PATH_MARKER}'");
        let said = self.ask(&script, self.given, until, |seen| marked_path(seen).is_some()).await?;
        marked_path(&said).ok_or(Unread::Unsaid)
    }

    /// What it prints for `script`, given `given`, until `said` holds for it, by `until`.
    async fn ask(
        &self,
        script: &str,
        given: &Environment,
        until: Instant,
        said: impl Fn(&[u8]) -> bool,
    ) -> Result<Vec<u8>, Unread> {
        let mut command = Command::new(self.shell);
        command
            .args(["-i", "-l", "-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if let Some(home) = self.home {
            command.current_dir(home);
        }
        given.apply(command.as_std_mut());
        let mut child = command.spawn().map_err(Unread::Unstarted)?;
        let stdout = child.stdout.take().ok_or(Unread::Unsaid)?;
        tokio::time::timeout_at(until, read_until(stdout, said)).await.map_err(|_| Unread::TimedOut)
    }
}

/// What `reader` gives until `said` holds for it, or it ends. Stops as soon as `said` holds rather than waiting for the
/// stream to end, because a process started by an rc file can hold it open indefinitely.
pub(super) async fn read_until<R: AsyncRead + Unpin>(mut reader: R, said: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    let mut seen = Vec::new();
    let mut chunk = [0u8; 4096];
    while let Ok(read) = reader.read(&mut chunk).await
        && read > 0
    {
        seen.extend_from_slice(&chunk[..read]);
        if said(&seen) {
            break;
        }
    }
    seen
}

/// The `PATH` between the two markers in what the shell printed, unless it's empty.
pub(super) fn marked_path(printed: &[u8]) -> Option<String> {
    let printed = String::from_utf8_lossy(printed);
    let start = printed.find(PATH_MARKER)? + PATH_MARKER.len();
    let end = start + printed[start..].find(PATH_MARKER)?;
    let path = printed[start..end].trim();
    (!path.is_empty()).then(|| path.to_string())
}
