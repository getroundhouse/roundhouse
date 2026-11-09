//! Watching a running process until it's gone, starting it again if the launch needs correcting.

use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use protocol::event::{EventBody, NoticeLevel};
use protocol::worker::AgentSpec;
use tokio::process::{Child, ChildStderr, ChildStdout};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::warn;

use super::launch::launch;
use super::output::{read_stderr, read_stdout};
use super::{Shared, drain_grace, write_to};
use crate::{AgentOptions, AgentOutput};

#[expect(clippy::too_many_arguments, reason = "its one caller hands over the parts of the process it just started")]
pub(super) async fn supervise(
    bin: PathBuf,
    spec: AgentSpec,
    options: AgentOptions,
    mut child: Child,
    mut stdout: ChildStdout,
    mut stderr: ChildStderr,
    shared: Arc<Shared>,
    output: mpsc::Sender<AgentOutput>,
) {
    let mut retried = false;
    let (status, stderr_tail) = loop {
        let out_task = tokio::spawn(read_stdout(stdout, shared.clone(), output.clone()));
        let err_task = tokio::spawn(read_stderr(stderr));
        let status = child.wait().await;
        // Before draining, which can take seconds. A launch that hasn't opened may yet be retried,
        // and until that's decided, starting another process would be wrong.
        if shared.confirmed.load(Ordering::SeqCst) {
            shared.gone.store(true, Ordering::SeqCst);
        }
        drain(out_task).await;
        let stderr_tail = drain(err_task).await.unwrap_or_default();

        // The hub decides between --session-id and --resume from its own records, and those can
        // disagree with the CLI's: a launch can crash right after the CLI saved the conversation,
        // or the user can delete it. Either way the CLI exits at once saying so, and the other flag
        // is right.
        let launch_mismatch =
            !shared.confirmed.load(Ordering::SeqCst) && !shared.stop_requested.load(Ordering::SeqCst) && !retried;
        let flip = launch_mismatch.then(|| classify_launch_failure(&stderr_tail)).flatten();
        let Some(failure) = flip else { break (status, stderr_tail) };
        retried = true;
        let resume = matches!(failure, LaunchFailure::AlreadyExists);
        shared.held_output.lock().clear();
        shared.pending_controls.lock().clear();
        if matches!(failure, LaunchFailure::NoConversation) {
            let notice = EventBody::Notice {
                level: NoticeLevel::Warning,
                message: "Claude's saved conversation for this session was not found, so it started a new one. \
                          The history here is intact, but the agent can't see it."
                    .to_string(),
            };
            let _ = output.send(AgentOutput::Event { body: notice, raw: None }).await;
        }
        let launched = match launch(&bin, &spec, resume, &options) {
            Ok(launched) => launched,
            Err(e) => {
                warn!("relaunch failed: {e:#}");
                break (status, vec![format!("{e:#}")]);
            }
        };
        shared.pid.store(launched.pid, Ordering::SeqCst);
        child = launched.child;
        stdout = launched.stdout;
        stderr = launched.stderr;
        let mut stdin = shared.stdin.lock().await;
        *stdin = Some(launched.stdin);
        let prompts = shared.unconfirmed_prompts.lock().clone();
        for prompt in prompts {
            if let Err(e) = write_to(&mut stdin, &prompt).await {
                warn!("could not replay a prompt after relaunching: {e:#}");
            }
        }
    };

    finish(&shared, &options, &output, status, stderr_tail).await;
}

/// Closes up after the last process has exited, then reports what it held back and how it ended.
async fn finish(
    shared: &Shared,
    options: &AgentOptions,
    output: &mpsc::Sender<AgentOutput>,
    status: std::io::Result<ExitStatus>,
    stderr_tail: Vec<String>,
) {
    shared.gone.store(true, Ordering::SeqCst);
    *shared.stdin.lock().await = None;
    shared.pending_controls.lock().clear();
    if let Some(path) = &options.pid_file {
        let _ = std::fs::remove_file(path);
    }
    let held: Vec<_> = shared.held_output.lock().drain(..).collect();
    for item in held {
        let _ = output.send(item).await;
    }
    let (code, signal) = status.map_or((None, None), |s| (s.code(), s.signal()));
    let requested = shared.stop_requested.load(Ordering::SeqCst);
    let _ = output.send(AgentOutput::Exited { code, signal, stderr_tail, requested }).await;
    shared.exited.send_replace(true);
}

#[derive(Debug, PartialEq)]
pub(super) enum LaunchFailure {
    /// `--resume` on a conversation that doesn't exist, so start it with `--session-id`.
    NoConversation,
    /// `--session-id` on a conversation that exists, so continue it with `--resume`.
    AlreadyExists,
}

pub(super) fn classify_launch_failure(stderr: &[String]) -> Option<LaunchFailure> {
    stderr.iter().find_map(|line| {
        if line.contains("No conversation found") {
            Some(LaunchFailure::NoConversation)
        } else if line.contains("is already in use") {
            Some(LaunchFailure::AlreadyExists)
        } else {
            None
        }
    })
}

async fn drain<T>(mut task: JoinHandle<T>) -> Option<T> {
    if let Ok(result) = tokio::time::timeout(drain_grace(), &mut task).await {
        result.ok()
    } else {
        task.abort();
        None
    }
}
