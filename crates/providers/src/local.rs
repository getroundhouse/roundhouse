//! Runs workers on this machine. Each one is a separate process connected over stdio, so a remote
//! worker later only changes the transport.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use process::{ProcessRecord, Signal, signal_group};
use protocol::model::InstanceId;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, OnceCell};
use tracing::{debug, info, warn};

use crate::{Capabilities, Provider, ProviderError, WorkerStream};

/// How long a worker gets to stop its agents after its stdin closes. Agents get 3 s each.
const STOP_GRACE: Duration = Duration::from_secs(10);
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(5);
/// How long an orphaned agent gets to stop by itself. It stops its own tools if given this.
const ORPHAN_GRACE: Duration = Duration::from_secs(3);
const PATH_MARKER: &str = "__ROUNDHOUSE_LOGIN_PATH__";

pub struct LocalConfig {
    /// The binary that runs `worker --stdio`.
    pub worker_exe: PathBuf,
    /// One directory per instance, holding its worker's log.
    pub instances_dir: PathBuf,
    pub workspaces_root: PathBuf,
    /// The user's shell, which has the real `PATH`. Found from the user's account if this is `None`.
    pub shell: Option<String>,
    /// This process's own `PATH`, which workers get as well as the login shell's.
    pub path: Option<String>,
}

struct Running {
    child: Child,
    stream: Option<WorkerStream>,
}

pub struct LocalProvider {
    config: LocalConfig,
    running: Mutex<HashMap<InstanceId, Running>>,
    login_path: Arc<OnceCell<Option<String>>>,
}

impl LocalProvider {
    pub fn new(config: LocalConfig) -> Self {
        let login_path = Arc::new(OnceCell::new());
        // Read it up front, so the first workspace doesn't pay for a slow shell startup.
        let early = login_path.clone();
        let shell = config.shell.clone();
        tokio::spawn(async move {
            early.get_or_init(|| read_login_path(shell)).await;
        });
        Self { config, running: Mutex::new(HashMap::new()), login_path }
    }

    fn instance_dir(&self, instance_id: &InstanceId) -> PathBuf {
        self.config.instances_dir.join(instance_id.as_str())
    }

    async fn end(&self, instance_id: &InstanceId, grace: Duration) -> Result<(), ProviderError> {
        let entry = self.running.lock().await.remove(instance_id);
        if let Some(mut entry) = entry {
            // Never connected, or the stream was given up: dropping it closes stdin either way.
            drop(entry.stream.take());
            if tokio::time::timeout(grace, entry.child.wait()).await.is_err() {
                if !grace.is_zero() {
                    warn!(%instance_id, "worker did not stop in time, killing it");
                }
                let _ = entry.child.kill().await;
            }
        }
        stop_orphaned_agents(&self.instance_dir(instance_id)).await;
        Ok(())
    }
}

/// An app started from Finder gets a minimal `PATH`, without Homebrew or anything from the user's
/// shell setup. The agent's tools need the real one, so ask the login shell for it.
async fn read_login_path(shell: Option<String>) -> Option<String> {
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
async fn read_until_second_marker<R: AsyncRead + Unpin>(mut reader: R) -> Option<String> {
    let mut seen = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = reader.read(&mut chunk).await.ok()?;
        if read == 0 {
            return Some(String::from_utf8_lossy(&seen).into_owned());
        }
        seen.extend_from_slice(&chunk[..read]);
        let text = String::from_utf8_lossy(&seen);
        if text.matches(PATH_MARKER).count() >= 2 {
            return Some(text.into_owned());
        }
    }
}

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

fn parse_marked_path(output: &str) -> Option<String> {
    let start = output.find(PATH_MARKER)? + PATH_MARKER.len();
    let end = start + output[start..].find(PATH_MARKER)?;
    let path = output[start..end].trim();
    (!path.is_empty()).then(|| path.to_string())
}

async fn still_running(agent: &ProcessRecord) -> bool {
    let agent = agent.clone();
    tokio::task::spawn_blocking(move || agent.is_running()).await.unwrap_or(false)
}

/// A worker records each agent it runs, and removes the record when that agent exits. Any record
/// still here means the worker was killed without stopping it. An orphan runs on until its turn ends,
/// with nobody reading its output, and a new agent would be working in the same worktree on the same
/// conversation. So it goes first.
async fn stop_orphaned_agents(instance_dir: &Path) {
    let Ok(records) = std::fs::read_dir(instance_dir.join("agents")) else {
        return;
    };
    for record in records.flatten() {
        let path = record.path();
        if let Some(agent) = ProcessRecord::read(&path).filter(|agent| agent.pid > 1)
            && still_running(&agent).await
        {
            let pid = agent.pid;
            warn!(pid, "stopping an agent left behind by a worker that was killed");
            signal_group(pid, Signal::Terminate);
            let deadline = tokio::time::Instant::now() + ORPHAN_GRACE;
            while still_running(&agent).await {
                if tokio::time::Instant::now() >= deadline {
                    warn!(pid, "it ignored SIGTERM, sending SIGKILL");
                    signal_group(pid, Signal::Kill);
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
        let _ = std::fs::remove_file(&path);
    }
}

/// The login shell's entries first, then anything else this process had. Ours may hold tools the
/// user added for this launch.
fn merge_paths(login: Option<&str>, own: Option<&str>) -> Option<String> {
    let mut seen = Vec::new();
    for entry in login.into_iter().chain(own).flat_map(|p| p.split(':')) {
        if !entry.is_empty() && !seen.contains(&entry) {
            seen.push(entry);
        }
    }
    (!seen.is_empty()).then(|| seen.join(":"))
}

#[async_trait]
impl Provider for LocalProvider {
    fn name(&self) -> &'static str {
        "local"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { filesystem_survives_stop: true, snapshots: false }
    }

    async fn create(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        let dir = self.instance_dir(instance_id);
        std::fs::create_dir_all(&dir).map_err(|e| ProviderError::io(format!("creating {}", dir.display()), e))
    }

    async fn start(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        let mut running = self.running.lock().await;
        if running.contains_key(instance_id) {
            return Err(ProviderError::AlreadyRunning(instance_id.clone()));
        }
        let dir = self.instance_dir(instance_id);
        std::fs::create_dir_all(&dir).map_err(|e| ProviderError::io(format!("creating {}", dir.display()), e))?;
        // For when the hub was killed along with its worker, so `stop` never ran.
        stop_orphaned_agents(&dir).await;
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("worker.log"))
            .map_err(|e| ProviderError::io("opening the worker log", e))?;

        let login = self.login_path.get_or_init(|| read_login_path(self.config.shell.clone())).await;
        let own = self.config.path.clone();
        let mut command = Command::new(&self.config.worker_exe);
        command
            .args(["worker", "--stdio", "--workspaces-root"])
            .arg(&self.config.workspaces_root)
            .arg("--state-dir")
            .arg(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log))
            .kill_on_drop(true);
        // So Ctrl-C on a foreground hub goes through the hub's clean shutdown.
        process::own_group(command.as_std_mut());
        if let Some(path) = merge_paths(login.as_deref(), own.as_deref()) {
            command.env("PATH", path);
        }
        let mut child = command
            .spawn()
            .map_err(|e| ProviderError::io(format!("starting worker {}", self.config.worker_exe.display()), e))?;
        let (Some(writer), Some(reader)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(ProviderError::NoPipes(self.config.worker_exe.display().to_string()));
        };
        info!(pid = child.id(), %instance_id, "started worker");
        let stream = WorkerStream { reader: Box::new(reader), writer: Box::new(writer) };
        running.insert(instance_id.clone(), Running { child, stream: Some(stream) });
        Ok(())
    }

    /// The caller closes the worker's stdin first, which is what asks it to stop. This reaps it.
    async fn stop(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.end(instance_id, STOP_GRACE).await
    }

    async fn kill(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.end(instance_id, Duration::ZERO).await
    }

    async fn connect(&self, instance_id: &InstanceId) -> Result<WorkerStream, ProviderError> {
        let mut running = self.running.lock().await;
        let entry = running.get_mut(instance_id).ok_or_else(|| ProviderError::NotRunning(instance_id.clone()))?;
        entry.stream.take().ok_or_else(|| ProviderError::AlreadyConnected(instance_id.clone()))
    }

    async fn destroy(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.stop(instance_id).await?;
        let dir = self.instance_dir(instance_id);
        match std::fs::remove_dir_all(&dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(ProviderError::io(format!("removing {}", dir.display()), e))
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn the_path_survives_whatever_rc_files_print_around_it() {
        let output = format!("Welcome!\n{PATH_MARKER}/a:/b\n{PATH_MARKER}\nbye\n");
        assert_eq!(parse_marked_path(&output).as_deref(), Some("/a:/b"));
        assert_eq!(parse_marked_path("no markers"), None);
        assert_eq!(parse_marked_path(&format!("{PATH_MARKER}{PATH_MARKER}")), None);
    }

    #[test]
    fn merging_keeps_login_entries_first_and_drops_duplicates() {
        assert_eq!(merge_paths(Some("/a:/b"), Some("/b:/c")).as_deref(), Some("/a:/b:/c"));
        assert_eq!(merge_paths(None, Some("/c")).as_deref(), Some("/c"));
        assert_eq!(merge_paths(None, None), None);
    }

    #[tokio::test]
    async fn the_login_shell_really_reports_a_path() {
        // Runs the real shell, so this also covers rc files that print to stdout.
        let path = read_login_path(None).await;
        assert!(path.as_deref().is_some_and(|p| p.contains('/')), "got {path:?}");
    }

    #[tokio::test]
    async fn reading_the_path_does_not_wait_for_a_stream_something_else_is_holding_open() {
        use tokio::io::AsyncWriteExt;

        let (mut writer, reader) = tokio::io::duplex(1024);
        writer.write_all(format!("noise {PATH_MARKER}/a:/b{PATH_MARKER}").as_bytes()).await.unwrap();
        // `writer` stays open, as it would with a background process started by an rc file.
        let out = tokio::time::timeout(Duration::from_secs(1), read_until_second_marker(reader)).await.unwrap();
        assert_eq!(out.as_deref().and_then(parse_marked_path).as_deref(), Some("/a:/b"));
    }

    fn orphan(dir: &Path) -> std::process::Child {
        use std::os::unix::process::CommandExt;
        std::process::Command::new("sleep").arg("100").process_group(0).current_dir(dir).spawn().unwrap()
    }

    fn record(dir: &Path, pid: u32, started: &str) -> PathBuf {
        std::fs::create_dir_all(dir.join("agents")).unwrap();
        let path = dir.join("agents/s.pid");
        std::fs::write(&path, format!("{pid}\n{started}\n")).unwrap();
        path
    }

    #[tokio::test]
    async fn a_recorded_orphan_is_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let mut child = orphan(dir.path());
        let started = process::start_time(child.id()).unwrap();
        let path = record(dir.path(), child.id(), &started);
        // It's this test's own child, so it stays a zombie until reaped, and `ps` still lists it.
        // Reap it from another thread so the wait below sees it go.
        let reaper = std::thread::spawn(move || child.wait().unwrap());
        stop_orphaned_agents(dir.path()).await;
        assert!(reaper.join().unwrap().signal().is_some(), "it should have been signalled");
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn a_pid_now_used_by_something_else_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let mut child = orphan(dir.path());
        let path = record(dir.path(), child.id(), "Thu Jan  1 00:00:00 1970");
        stop_orphaned_agents(dir.path()).await;
        assert!(child.try_wait().unwrap().is_none(), "this isn't the process that was recorded");
        assert!(!path.exists(), "the stale record still goes");
        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[tokio::test]
    async fn stop_and_destroy_are_safe_on_unknown_instances() {
        let dir = tempfile::tempdir().unwrap();
        let provider = LocalProvider::new(LocalConfig {
            worker_exe: "/nonexistent".into(),
            instances_dir: dir.path().join("instances"),
            workspaces_root: dir.path().join("workspaces"),
            shell: None,
            path: None,
        });
        provider.stop(&"nope".into()).await.unwrap();
        provider.destroy(&"nope".into()).await.unwrap();
        assert!(provider.connect(&"nope".into()).await.is_err());
    }
}
