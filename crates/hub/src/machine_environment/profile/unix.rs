//! The profile's environment on Unix, as the user's login shell has it once it has read their profile.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use process::Environment;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;

use super::super::Vars;
use super::super::printing::{PRINTER, PRINTS_ENVIRONMENT};
use super::entries::{from_shell, marked};

/// How long the login shell gets to say its environment, since rc files can run anything.
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// Every variable the user's login shell, `shell` or else their account's, exports once it has read their profile,
/// started as an interactive login shell, as a terminal app starts one. The shell runs `printer`, the daemon, which
/// prints what it was started with, as [`print_environment`](super::super::print_environment) says. It runs in `session`
/// alone, so nothing else of whoever started the hub comes back through it, such as a `PATH` a profile adds to. `None`
/// if it fails, says nothing within [`LOGIN_SHELL_TIMEOUT`], or says nothing of the variables it has.
pub(in crate::machine_environment) async fn read(shell: Option<&str>, session: &Vars, printer: &Path) -> Option<Vars> {
    let shell = shell.map(str::to_string).or_else(account_shell)?;
    // The daemon rather than `env -0`, which not every system's `env` takes. Its path is in a variable, which every
    // shell expands the same in double quotes, fish's too, where quoting the path itself differs from shell to shell. It
    // prints the markers too, as `MARKER` says why.
    let script = format!("\"${PRINTER}\" {PRINTS_ENVIRONMENT}");
    let mut command = Command::new(&shell);
    command
        .args(["-i", "-l", "-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let given = session.iter().map(|(name, value)| (OsStr::new(name), OsStr::new(value)));
    let given = given.chain([(OsStr::new(PRINTER), printer.as_os_str())]);
    Environment::whole(given).apply(command.as_std_mut());
    let mut child = command.spawn().ok()?;
    let stdout = child.stdout.take()?;
    let said = tokio::time::timeout(LOGIN_SHELL_TIMEOUT, read_until_second_marker(stdout)).await.ok().flatten()?;
    from_shell(&said).filter(|vars| !vars.is_empty())
}

/// Stops as soon as both markers have arrived rather than waiting for the stream to end, because a process started by
/// an rc file can hold it open indefinitely.
pub(in crate::machine_environment) async fn read_until_second_marker<R: AsyncRead + Unpin>(
    mut reader: R,
) -> Option<Vec<u8>> {
    let mut seen = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = reader.read(&mut chunk).await.ok()?;
        if read == 0 {
            return Some(seen);
        }
        seen.extend_from_slice(&chunk[..read]);
        if marked(&seen).is_some() {
            return Some(seen);
        }
    }
}

fn account_shell() -> Option<String> {
    // SAFETY: getpwuid returns a pointer to static storage or null. The string is copied before anything else can
    // overwrite it, and this runs on one thread at a time in this process.
    unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() || (*entry).pw_shell.is_null() {
            return None;
        }
        Some(std::ffi::CStr::from_ptr((*entry).pw_shell).to_string_lossy().into_owned())
    }
}
