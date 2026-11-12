//! Runs workers on this machine. Each one is a separate process, which the hub starts with the command `connect`
//! answers with, so a remote worker only changes the command.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use process::{ProcessRecord, Signal, signal_group};
use protocol::model::InstanceId;
use protocol::paths::WORKER_RECORD_FILE;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::OnceCell;
use tracing::{debug, warn};

use crate::{
    Capabilities, Feature, Instance, InstanceState, Provider, ProviderError, Size, WorkerBinary, WorkerCommand,
};

const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(5);
/// How long an orphaned agent, or a worker still running when its instance is stopped, gets to stop by itself. An
/// agent stops its own tools if given this.
const ORPHAN_GRACE: Duration = Duration::from_secs(3);
const PATH_MARKER: &str = "__ROUNDHOUSE_LOGIN_PATH__";
/// How long a process sent SIGKILL gets to be gone.
const KILLED_GRACE: Duration = Duration::from_secs(2);

pub struct LocalConfig {
    /// The binary that runs `worker --stdio`.
    pub worker_exe: PathBuf,
    /// One directory per instance, holding its worker's records.
    pub instances_dir: PathBuf,
    pub workspaces_root: PathBuf,
    /// The user's shell, which has the real `PATH`. Found from the user's account if this is `None`.
    pub shell: Option<String>,
    /// This process's own `PATH`, which workers get as well as the login shell's.
    pub path: Option<String>,
}

pub struct LocalProvider {
    config: LocalConfig,
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
        Self { config, login_path }
    }

    fn instance_dir(&self, instance_id: &InstanceId) -> PathBuf {
        self.config.instances_dir.join(instance_id.as_str())
    }

    /// Ends the instance's worker, if one is still running, and then any agent a worker left behind.
    async fn end(&self, instance_id: &InstanceId, grace: Duration) {
        let dir = self.instance_dir(instance_id);
        if let Some(worker) = running_worker(&dir).await {
            warn!(pid = worker.pid, %instance_id, "ending a worker still running in an instance that's stopping");
            stop_group(&worker, grace).await;
        }
        stop_orphaned_agents(&dir).await;
    }
}

/// The worker serving the instance in `dir`, which records itself there once it holds the instance's lock.
async fn running_worker(dir: &Path) -> Option<ProcessRecord> {
    let worker = ProcessRecord::read(&dir.join(WORKER_RECORD_FILE)).filter(|worker| worker.pid > 1)?;
    still_running(&worker).await.then_some(worker)
}

/// Whether the process exits by itself within `grace`.
async fn exits_within(process: &ProcessRecord, grace: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + grace;
    while still_running(process).await {
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    true
}

/// Asks the process group to stop, and makes it after `grace`, or at once if that's zero. Returns once its leader
/// has gone.
async fn stop_group(leader: &ProcessRecord, grace: Duration) {
    if !grace.is_zero() {
        signal_group(leader.pid, Signal::Terminate);
        if exits_within(leader, grace).await {
            return;
        }
        warn!(pid = leader.pid, "it ignored SIGTERM, sending SIGKILL");
    }
    signal_group(leader.pid, Signal::Kill);
    if !exits_within(leader, KILLED_GRACE).await {
        warn!(pid = leader.pid, "it's still running after SIGKILL");
    }
}

fn create_dir(dir: &Path) -> Result<(), ProviderError> {
    std::fs::create_dir_all(dir).map_err(|e| ProviderError::io(format!("creating {}", dir.display()), e))
}

/// An app started from Finder gets a minimal `PATH`, without Homebrew or anything from the user's
/// shell setup. The agent's tools need the real one, so ask the login shell for it.
async fn read_login_path(shell: Option<String>) -> Option<String> {
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
            warn!(pid = agent.pid, "stopping an agent left behind by a worker that was killed");
            stop_group(&agent, ORPHAN_GRACE).await;
        }
        let _ = std::fs::remove_file(&path);
    }
}

/// The login shell's entries first, then anything else this process had. Ours may hold tools the
/// user added for this launch.
fn merge_paths(login: Option<&str>, own: Option<&str>) -> Option<String> {
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

#[async_trait]
impl Provider for LocalProvider {
    fn name(&self) -> &'static str {
        "local"
    }

    /// Its workers run the hub's own executable, so it names no targets to be given workers for.
    fn capabilities(&self) -> Capabilities {
        Capabilities { targets: Vec::new(), features: vec![Feature::FilesystemSurvivesStop] }
    }

    async fn create(
        &self,
        instance_id: &InstanceId,
        from: Option<&InstanceId>,
        _size: Option<Size>,
        _display_name: &str,
    ) -> Result<(), ProviderError> {
        if from.is_some() {
            return Err(ProviderError::Unsupported("make an instance from a base image"));
        }
        create_dir(&self.instance_dir(instance_id))
    }

    /// Its workers run the hub's own executable, so there's none to put in place. A running instance is left as it
    /// is, and one is running while a worker is. Ending that is the hub's to ask for, with `stop`.
    async fn start(&self, instance_id: &InstanceId, _workers: &[WorkerBinary]) -> Result<(), ProviderError> {
        let dir = self.instance_dir(instance_id);
        create_dir(&dir)?;
        if running_worker(&dir).await.is_none() {
            // For when the hub was killed along with its worker, so `stop` never ran.
            stop_orphaned_agents(&dir).await;
        }
        Ok(())
    }

    async fn stop(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.end(instance_id, ORPHAN_GRACE).await;
        Ok(())
    }

    async fn kill(&self, instance_id: &InstanceId) -> Result<(), ProviderError> {
        self.end(instance_id, Duration::ZERO).await;
        Ok(())
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

    async fn list(&self) -> Result<Vec<Instance>, ProviderError> {
        let dir = &self.config.instances_dir;
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(ProviderError::io(format!("reading {}", dir.display()), e)),
        };
        let mut instances = Vec::new();
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let state = match running_worker(&entry.path()).await {
                Some(_) => InstanceState::Running,
                None => InstanceState::Stopped,
            };
            let instance_id = entry.file_name().to_string_lossy().into_owned().into();
            instances.push(Instance { instance_id, state, base_image: false });
        }
        Ok(instances)
    }

    async fn connect(&self, instance_id: &InstanceId) -> Result<WorkerCommand, ProviderError> {
        let dir = self.instance_dir(instance_id);
        if !dir.is_dir() {
            return Err(ProviderError::NotFound(instance_id.clone()));
        }
        let login = self.login_path.get_or_init(|| read_login_path(self.config.shell.clone())).await;
        let env = merge_paths(login.as_deref(), self.config.path.as_deref())
            .map(|path| ("PATH".to_string(), path))
            .into_iter()
            .collect();
        let text = |path: &Path| path.to_string_lossy().into_owned();
        Ok(WorkerCommand {
            program: text(&self.config.worker_exe),
            args: vec![
                "worker".into(),
                "--stdio".into(),
                "--workspaces-root".into(),
                text(&self.config.workspaces_root),
                "--state-dir".into(),
                text(&dir),
            ],
            env,
        })
    }

    async fn make_base_image(&self, _instance_id: &InstanceId) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported("make base images"))
    }
}

#[cfg(test)]
mod tests;
