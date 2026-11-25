use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::ExitStatus;

use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::process::{ChildStderr, ChildStdout};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::warn;

use crate::DrainGrace;

const STDERR_TAIL_LINES: usize = 20;

/// Reads bytes rather than text: one line of invalid UTF-8 mustn't end the whole stream, which would
/// leave the agent blocked and the session stuck.
pub(crate) async fn next_line<R: AsyncBufRead + Unpin>(reader: &mut R, buffer: &mut Vec<u8>) -> Option<String> {
    buffer.clear();
    match reader.read_until(b'\n', buffer).await {
        Ok(0) => None,
        Ok(_) => Some(String::from_utf8_lossy(buffer).trim_end_matches(['\n', '\r']).to_string()),
        Err(e) => {
            warn!(name: "agent.output_unread", error = %e, "Couldn't read an agent's output");
            None
        }
    }
}

pub(crate) async fn read_json_lines(
    stdout: ChildStdout,
    tap: Option<crate::tap::Tap>,
    notifications: mpsc::Sender<(Value, String)>,
) {
    let mut reader = BufReader::new(stdout);
    let mut buffer = Vec::new();
    while let Some(raw) = next_line(&mut reader, &mut buffer).await {
        if let Some(tap) = &tap {
            tap.read(&raw);
        }
        let Ok(value) = serde_json::from_str::<Value>(&raw) else { continue };
        if notifications.send((value, raw)).await.is_err() {
            break;
        }
    }
}

pub(crate) async fn drain<T>(mut task: JoinHandle<T>, DrainGrace(grace): DrainGrace) -> Option<T> {
    if let Ok(result) = tokio::time::timeout(grace, &mut task).await {
        result.ok()
    } else {
        task.abort();
        let _ = task.await;
        None
    }
}

pub(crate) async fn read_stderr(stderr: ChildStderr) -> Vec<String> {
    let mut reader = BufReader::new(stderr);
    let mut buffer = Vec::new();
    let mut tail = VecDeque::with_capacity(STDERR_TAIL_LINES);
    while let Some(line) = next_line(&mut reader, &mut buffer).await {
        if tail.len() == STDERR_TAIL_LINES {
            tail.pop_front();
        }
        tail.push_back(line);
    }
    tail.into()
}

/// A CLI that's been started, such as for a one-off call, recorded at `pid_file`, if it's given one, so the provider
/// can stop it if the worker is killed. Unless marked finished, dropping this guard kills the whole process group.
pub(crate) struct GroupGuard {
    pid: u32,
    pid_file: Option<PathBuf>,
    finished: bool,
}

impl GroupGuard {
    pub(crate) fn new(pid: u32, pid_file: Option<PathBuf>) -> Self {
        if let Some(path) = &pid_file
            && let Err(error) = process::ProcessRecord::write(path, pid)
        {
            warn!(
                name: "agent.process_unrecorded",
                pid,
                path = %path.display(),
                %error,
                "Couldn't record a process, so it's left running if the worker is killed"
            );
        }
        Self { pid, pid_file, finished: false }
    }

    pub(crate) fn finished(mut self) {
        self.finished = true;
    }
}

impl Drop for GroupGuard {
    fn drop(&mut self) {
        if !self.finished {
            process::signal_group(self.pid, process::Signal::Kill);
        }
        if let Some(path) = &self.pid_file {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(unix)]
pub(crate) fn signal_of(status: ExitStatus) -> Option<i32> {
    std::os::unix::process::ExitStatusExt::signal(&status)
}

/// Windows reports exit codes, not signals.
#[cfg(windows)]
pub(crate) fn signal_of(_status: ExitStatus) -> Option<i32> {
    None
}
