//! The `PATH` of the machine the hub runs on, as its user's login shell has it, which the local provider's workers are
//! given, and which the hub runs `gh` with. An app started from Finder gets a minimal `PATH`, without Homebrew or
//! anything from the user's shell setup, and the agents' tools and `gh` need the real one.

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::OnceCell;
use tracing::{debug, warn};

/// How long the login shell gets to say its `PATH`, since rc files can run anything.
pub(super) const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const PATH_MARKER: &str = "__ROUNDHOUSE_LOGIN_PATH__";

/// The machine's `PATH`: the login shell's, read once, as it's made, followed by this process's own. Clones share the
/// one read.
#[derive(Clone)]
pub struct MachinePath {
    /// The user's shell, which has the real `PATH`. Found from the user's account if this is `None`.
    shell: Option<String>,
    /// This process's own `PATH`, which may hold tools the user added for this launch.
    own: Option<String>,
    login: Arc<OnceCell<Option<String>>>,
}

impl MachinePath {
    /// Reads the login shell's `PATH` up front, on a task of its own, so the first workspace, or the first run of
    /// `gh`, doesn't pay for a slow shell startup.
    pub fn new(shell: Option<String>, own: Option<String>) -> Self {
        let path = Self { shell, own, login: Arc::new(OnceCell::new()) };
        let early = path.clone();
        tokio::spawn(async move {
            early.login().await;
        });
        path
    }

    /// Only `own`, with no login shell asked, for a hub whose `PATH` is known, such as a test's.
    pub fn only(own: Option<String>) -> Self {
        Self { shell: None, own, login: Arc::new(OnceCell::new_with(Some(None))) }
    }

    /// The login shell's entries first, then this process's own, once the login shell has said, or within
    /// [`LOGIN_SHELL_TIMEOUT`].
    pub async fn path(&self) -> Option<String> {
        merge_paths(self.login().await.as_deref(), self.own.as_deref())
    }

    async fn login(&self) -> &Option<String> {
        self.login.get_or_init(|| read_login_path(self.shell.clone())).await
    }
}

/// An app started from Finder gets a minimal `PATH`, without Homebrew or anything from the user's
/// shell setup. The agent's tools need the real one, so ask the login shell for it.
pub(super) async fn read_login_path(shell: Option<String>) -> Option<String> {
    // Every process on Windows gets the user's whole `PATH` already, and a shell that could be asked there, such
    // as Git's, would give it in a form of its own.
    if cfg!(windows) {
        return None;
    }
    let shell = shell.or_else(account_shell)?;
    // rc files can print anything, so the value is marked. `printenv` works in every shell, which
    // matters for fish, where `$PATH` expands to a list.
    let script = format!("printf '{PATH_MARKER}'; printenv PATH; printf '{PATH_MARKER}'");
    let mut command = Command::new(&shell);
    command
        .args(["-i", "-l", "-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn().ok()?;
    let stdout = child.stdout.take()?;
    let out = tokio::time::timeout(LOGIN_SHELL_TIMEOUT, read_until_second_marker(stdout)).await.ok().flatten();
    let path = out.as_deref().and_then(parse_marked_path);
    if path.is_some() {
        debug!("read PATH from login shell {shell}");
    } else {
        warn!("could not read PATH from login shell {shell}");
    }
    path
}

/// Stops as soon as both markers have arrived rather than waiting for the stream to end, because a
/// process started by an rc file can hold it open indefinitely.
pub(super) async fn read_until_second_marker<R: AsyncRead + Unpin>(mut reader: R) -> Option<String> {
    let mut seen = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = reader.read(&mut chunk).await.ok()?;
        if read == 0 {
            return Some(String::from_utf8_lossy_owned(seen));
        }
        seen.extend_from_slice(&chunk[..read]);
        let text = String::from_utf8_lossy(&seen);
        if text.matches(PATH_MARKER).count() >= 2 {
            return Some(text.into_owned());
        }
    }
}

/// Windows gives an account no shell.
#[cfg(windows)]
fn account_shell() -> Option<String> {
    None
}

#[cfg(unix)]
fn account_shell() -> Option<String> {
    // SAFETY: getpwuid returns a pointer to static storage or null. The string is copied before
    // anything else can overwrite it, and this runs on one thread at a time in this process.
    unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() || (*entry).pw_shell.is_null() {
            return None;
        }
        Some(std::ffi::CStr::from_ptr((*entry).pw_shell).to_string_lossy().into_owned())
    }
}

pub(super) fn parse_marked_path(output: &str) -> Option<String> {
    let start = output.find(PATH_MARKER)? + PATH_MARKER.len();
    let end = start + output[start..].find(PATH_MARKER)?;
    let path = output[start..end].trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// The login shell's entries first, then anything else this process had. Ours may hold tools the
/// user added for this launch.
pub(super) fn merge_paths(login: Option<&str>, own: Option<&str>) -> Option<String> {
    let mut seen = Vec::new();
    for entry in login.into_iter().chain(own).flat_map(std::env::split_paths) {
        if !entry.as_os_str().is_empty() && !seen.contains(&entry) {
            seen.push(entry);
        }
    }
    if seen.is_empty() {
        return None;
    }
    // Both came in as text and were split where a `PATH` is, so the entries join again, and into text.
    std::env::join_paths(seen).ok()?.into_string().ok()
}
