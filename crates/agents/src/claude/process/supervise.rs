//! Watching a running process until it's gone, starting it again if the launch needs correcting.

use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::claude::login_check::Output;
use protocol::event::EventBody;
use protocol::model::PermissionMode;
use protocol::worker::AgentSpec;
use tokio::process::{Child, ChildStdin};
use tokio::sync::mpsc;
use tokio::task::{JoinHandle, JoinSet};
use tracing::warn;

use super::output::read_stdout;
use super::run_files::RunFiles;
use super::verified_launch::launch;
use super::{Shared, drain_grace, plan, write_to};
use crate::stdio::signal_of;
use crate::{AgentOptions, AgentOutput};

#[expect(clippy::too_many_arguments, reason = "its one caller hands over the parts of the process it just started")]
pub(super) async fn supervise(
    bin: PathBuf,
    spec: AgentSpec,
    mode: protocol::model::PermissionMode,
    options: AgentOptions,
    files: RunFiles,
    mut recovery: super::recovery::Recovery,
    mut child: Child,
    mut stdout: Output,
    mut stderr: JoinSet<Vec<String>>,
    shared: Arc<Shared>,
    output: mpsc::Sender<AgentOutput>,
) {
    let (status, stderr_tail) = loop {
        let out_task = tokio::spawn(read_stdout(stdout, shared.clone(), output.clone()));
        let err_task = stderr;
        let status = child.wait().await;
        // No stop may signal a reaped PID while a replacement is being verified.
        shared.pid.store(0, Ordering::SeqCst);
        // Before draining, which can take seconds. A launch that hasn't opened may yet be retried,
        // and until that's decided, starting another process would be wrong.
        if shared.confirmed.load(Ordering::SeqCst) {
            shared.gone.store(true, Ordering::SeqCst);
        }
        drain(out_task).await;
        let stderr_tail = drain_errors(err_task).await;

        // The hub decides between --session-id and --resume from its own records, and those can
        // disagree with the CLI's: a launch can crash right after the CLI saved the conversation,
        // or the user can delete it. Either way the CLI exits at once saying so, and the other flag
        // is right.
        if shared.confirmed.load(Ordering::SeqCst) || shared.stop_requested.load(Ordering::SeqCst) {
            break (status, stderr_tail);
        }
        let Some(resume) = recovery.correct(&stderr_tail, &output).await else { break (status, stderr_tail) };
        shared.held_output.lock().clear();
        shared.pending_controls.lock().clear();
        shared.context.lock().relaunched();
        shared.usage.lock().relaunched();
        if let Some(tap) = &shared.tap {
            let (code, signal) = status.as_ref().map_or((None, None), |s| (s.code(), signal_of(*s)));
            tap.exited(code, signal, false);
        }
        // Verification is before prompt replay and can wait on the CLI. Stop cancels it, whose
        // startup guard kills the replacement group, even before it has become the current process.
        let replacement = tokio::select! {
            biased;
            () = shared.stopping.notified() => break (status, stderr_tail),
            launched = launch(&bin, &spec, resume, &options, shared.tap.as_ref(), &output, &mut recovery) => launched,
        };
        let mut launched = match replacement {
            Ok(launched) => launched,
            Err(e) => {
                warn!("relaunch failed: {e:#}");
                break (status, vec![format!("{e:#}")]);
            }
        };
        if shared.stop_requested.load(Ordering::SeqCst) {
            process::signal_group(launched.pid, process::Signal::Kill);
            let _ = launched.child.wait().await;
            break (status, stderr_tail);
        }
        shared.pid.store(launched.pid, Ordering::SeqCst);
        child = launched.child;
        stdout = launched.stdout;
        stderr = launched.stderr;
        let mut stdin = shared.stdin.lock().await;
        *stdin = Some(launched.stdin);
        if shared.stop_requested.load(Ordering::SeqCst) {
            process::signal_group(launched.pid, process::Signal::Terminate);
            continue;
        }
        replay(mode, &shared, &mut stdin, &output).await;
    };

    finish(&shared, files, &output, status, stderr_tail).await;
}

/// Reestablishes the permission mode before replaying prompts the previous process never took.
async fn replay(
    mode: PermissionMode,
    shared: &Arc<Shared>,
    stdin: &mut Option<ChildStdin>,
    output: &mpsc::Sender<AgentOutput>,
) {
    plan::start_in(mode, shared, stdin, output).await;
    let prompts = shared.unconfirmed_prompts.lock().clone();
    for prompt in prompts {
        if shared.stop_requested.load(Ordering::SeqCst) {
            break;
        }
        if let Err(e) = write_to(shared.tap.as_ref(), stdin, &prompt).await {
            warn!("could not replay a prompt after relaunching: {e:#}");
        }
    }
}

/// Closes up after the last process has exited, then reports what it held back and how it ended.
async fn finish(
    shared: &Shared,
    files: RunFiles,
    output: &mpsc::Sender<AgentOutput>,
    status: std::io::Result<ExitStatus>,
    stderr_tail: Vec<String>,
) {
    shared.gone.store(true, Ordering::SeqCst);
    *shared.stdin.lock().await = None;
    shared.pending_controls.lock().clear();
    drop(files);
    let held: Vec<_> = shared.held_output.lock().drain(..).collect();
    for item in held {
        let _ = output.send(item).await;
    }
    // The CLI never took these, and nothing will now.
    let mut untaken: Vec<_> = shared.untaken.lock().drain().map(|(_, prompt)| prompt).collect();
    untaken.sort_unstable();
    for prompt in untaken {
        let _ = output.send(AgentOutput::event(EventBody::PromptDropped { prompt }, None)).await;
    }
    let (code, signal) = status.map_or((None, None), |s| (s.code(), signal_of(s)));
    let requested = shared.stop_requested.load(Ordering::SeqCst);
    if let Some(tap) = &shared.tap {
        tap.exited(code, signal, requested);
    }
    shared.context.lock().close();
    shared.usage.lock().close();
    let _ = output.send(AgentOutput::Exited { code, signal, stderr_tail, requested }).await;
    shared.exited.send_replace(true);
}

async fn drain<T>(mut task: JoinHandle<T>) -> Option<T> {
    if let Ok(result) = tokio::time::timeout(drain_grace(), &mut task).await {
        result.ok()
    } else {
        task.abort();
        None
    }
}

/// Dropping the set aborts its reader, including when startup verification is cancelled.
pub(super) async fn drain_errors(mut tasks: JoinSet<Vec<String>>) -> Vec<String> {
    match tokio::time::timeout(drain_grace(), tasks.join_next()).await {
        Ok(Some(Ok(tail))) => tail,
        _ => Vec::new(),
    }
}
