//! Runs workers on this machine. Each one is a separate process connected over stdio, so a remote
//! worker later only changes the transport.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, OnceCell};
use tracing::{debug, info, warn};

use crate::{Capabilities, Provider, WorkerStream};

/// How long a worker gets to stop its agents after its stdin closes. Agents get 3 s each.
const STOP_GRACE: Duration = Duration::from_secs(10);
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(5);
/// How long an orphaned agent gets to stop by itself. It stops its own tools if given this.
const ORPHAN_GRACE: Duration = Duration::from_secs(3);
const PATH_MARKER: &str = "__TRESTLE_LOGIN_PATH__";

pub struct LocalConfig {
    /// The binary that runs `worker --stdio`.
    pub worker_exe: PathBuf,
    /// One directory per instance, holding its worker's log.
    pub instances_dir: PathBuf,
    pub workspaces_root: PathBuf,
}

struct Running {
    child: Child,
    stream: Option<WorkerStream>,
}

pub struct LocalProvider {
    config: LocalConfig,
    running: Mutex<HashMap<String, Running>>,
    login_path: Arc<OnceCell<Option<String>>>,
}

impl LocalProvider {
    pub fn new(config: LocalConfig) -> Self {
        let login_path = Arc::new(OnceCell::new());
        // Read it up front, so the first workspace doesn't pay for a slow shell startup.
        let early = login_path.clone();
        tokio::spawn(async move {
            early.get_or_init(read_login_path).await;
        });
        Self { config, running: Mutex::new(HashMap::new()), login_path }
    }

    fn instance_dir(&self, instance_id: &str) -> PathBuf {
        self.config.instances_dir.join(instance_id)
    }

    async fn end(&self, instance_id: &str, grace: Duration) -> anyhow::Result<()> {
        let entry = self.running.lock().await.remove(instance_id);
        if let Some(mut entry) = entry {
            // Never connected, or the stream was given up: dropping it closes stdin either way.
            drop(entry.stream.take());
            if tokio::time::timeout(grace, entry.child.wait()).await.is_err() {
                if !grace.is_zero() {
                    warn!(instance_id, "worker did not stop in time, killing it");
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
async fn read_login_path() -> Option<String> {
    let shell = login_shell()?;
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
    match &path {
        Some(_) => debug!("read PATH from login shell {shell}"),
        None => warn!("could not read PATH from login shell {shell}"),
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

fn login_shell() -> Option<String> {
    if let Some(shell) = std::env::var("SHELL").ok().filter(|s| !s.is_empty()) {
        return Some(shell);
    }
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

async fn process_start_time(pid: u32) -> Option<String> {
    let output = Command::new("ps").args(["-o", "lstart=", "-p", &pid.to_string()]).output().await.ok()?;
    let started = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!started.is_empty()).then_some(started)
}

fn signal_group(pid: u32, signal: i32) {
    // SAFETY: kill(2) has no memory effects. The caller has just confirmed by start time that this is
    // the process that was recorded, and it was started as a group leader.
    unsafe { libc::kill(-(pid as i32), signal) };
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
        let contents = std::fs::read_to_string(&path).unwrap_or_default();
        let mut lines = contents.lines();
        let pid = lines.next().and_then(|line| line.parse::<u32>().ok());
        let recorded_start = lines.next().unwrap_or_default();
        if let Some(pid) = pid.filter(|pid| *pid > 1)
            && !recorded_start.is_empty()
            && process_start_time(pid).await.as_deref() == Some(recorded_start)
        {
            warn!(pid, "stopping an agent left behind by a worker that was killed");
            signal_group(pid, libc::SIGTERM);
            let deadline = tokio::time::Instant::now() + ORPHAN_GRACE;
            while process_start_time(pid).await.as_deref() == Some(recorded_start) {
                if tokio::time::Instant::now() >= deadline {
                    warn!(pid, "it ignored SIGTERM, sending SIGKILL");
                    signal_group(pid, libc::SIGKILL);
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
    fn name(&self) -> &str {
        "local"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities { filesystem_survives_stop: true, snapshots: false }
    }

    async fn create(&self, instance_id: &str) -> anyhow::Result<()> {
        let dir = self.instance_dir(instance_id);
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))
    }

    async fn start(&self, instance_id: &str) -> anyhow::Result<()> {
        let mut running = self.running.lock().await;
        if running.contains_key(instance_id) {
            return Err(anyhow!("instance {instance_id} is already running"));
        }
        let dir = self.instance_dir(instance_id);
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        // For when the hub was killed along with its worker, so `stop` never ran.
        stop_orphaned_agents(&dir).await;
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("worker.log"))
            .context("opening the worker log")?;

        let login = self.login_path.get_or_init(read_login_path).await;
        let own = std::env::var("PATH").ok();
        let mut command = Command::new(&self.config.worker_exe);
        command
            .args(["worker", "--stdio", "--workspaces-root"])
            .arg(&self.config.workspaces_root)
            .arg("--state-dir")
            .arg(&dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log))
            // Its own group, so Ctrl-C on a foreground hub goes through the hub's clean shutdown.
            .process_group(0)
            .kill_on_drop(true);
        if let Some(path) = merge_paths(login.as_deref(), own.as_deref()) {
            command.env("PATH", path);
        }
        let mut child =
            command.spawn().with_context(|| format!("starting worker {}", self.config.worker_exe.display()))?;
        let writer = child.stdin.take().expect("stdin was piped");
        let reader = child.stdout.take().expect("stdout was piped");
        info!(pid = child.id(), instance_id, "started worker");
        let stream = WorkerStream { reader: Box::new(reader), writer: Box::new(writer) };
        running.insert(instance_id.to_string(), Running { child, stream: Some(stream) });
        Ok(())
    }

    /// The caller closes the worker's stdin first, which is what asks it to stop. This reaps it.
    async fn stop(&self, instance_id: &str) -> anyhow::Result<()> {
        self.end(instance_id, STOP_GRACE).await
    }

    async fn kill(&self, instance_id: &str) -> anyhow::Result<()> {
        self.end(instance_id, Duration::ZERO).await
    }

    async fn connect(&self, instance_id: &str) -> anyhow::Result<WorkerStream> {
        let mut running = self.running.lock().await;
        let entry = running.get_mut(instance_id).ok_or_else(|| anyhow!("instance {instance_id} isn't running"))?;
        entry.stream.take().ok_or_else(|| anyhow!("instance {instance_id} already has a connection"))
    }

    async fn destroy(&self, instance_id: &str) -> anyhow::Result<()> {
        self.stop(instance_id).await?;
        let dir = self.instance_dir(instance_id);
        match std::fs::remove_dir_all(&dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(e).with_context(|| format!("removing {}", dir.display()))
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
        let path = read_login_path().await;
        assert!(path.as_deref().is_some_and(|p| p.contains('/')), "got {path:?}");
    }

    #[tokio::test]
    async fn reading_the_path_does_not_wait_for_a_stream_something_else_is_holding_open() {
        let (mut writer, reader) = tokio::io::duplex(1024);
        use tokio::io::AsyncWriteExt;
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
        let started = process_start_time(child.id()).await.unwrap();
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
        });
        provider.stop("nope").await.unwrap();
        provider.destroy("nope").await.unwrap();
        assert!(provider.connect("nope").await.is_err());
    }
}
