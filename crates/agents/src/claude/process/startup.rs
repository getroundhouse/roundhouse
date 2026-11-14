//! Construct the process state only after its initial login has been verified.

use parking_lot::Mutex;
use protocol::worker::AgentSpec;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32};
use tokio::sync::mpsc;

use super::compact::AskedToCompact;
use super::context::ContextWatch;
use super::run_files::RunFiles;
use super::running::{ClaudeProcess, Shared};
use super::usage::UsageWatch;
use super::{Tap, launch, plan, supervise};
use crate::{AgentError, AgentOptions, AgentOutput, AgentProcess};

pub(super) async fn begin(
    bin: PathBuf,
    spec: AgentSpec,
    output: mpsc::Sender<AgentOutput>,
    options: AgentOptions,
    tap: Option<Tap>,
) -> Result<Arc<dyn AgentProcess>, AgentError> {
    let files = RunFiles::of(&options);
    let mut recovery = super::recovery::Recovery::default();
    let launched = launch(
        &bin,
        &spec,
        matches!(spec.start, protocol::capability::conversation::Start::Resume { .. }),
        &options,
        tap.as_ref(),
        &output,
        &mut recovery,
    )
    .await?;
    let shared = Arc::new(Shared {
        stdin: tokio::sync::Mutex::new(Some(launched.stdin)),
        pid: AtomicU32::new(launched.pid),
        confirmed: AtomicBool::new(false),
        held_output: Mutex::new(Vec::new()),
        unconfirmed_prompts: Mutex::new(Vec::new()),
        untaken: Mutex::new(HashMap::new()),
        compacts: Mutex::new(AskedToCompact::default()),
        waiting: Mutex::new(HashMap::new()),
        pending_controls: Mutex::new(HashMap::new()),
        context: Mutex::new(ContextWatch::default()),
        usage: Mutex::new(UsageWatch::default()),
        stop_requested: AtomicBool::new(false),
        stopping: tokio::sync::Notify::new(),
        entering_plan: AtomicBool::new(false),
        gone: AtomicBool::new(false),
        exited: tokio::sync::watch::channel(false).0,
        tap,
    });
    // Converted where it's used, as `build_args` does, rather than carrying a second form of the settings through the
    // launch, its retries and supervision, until the adapters take typed settings in #1274.
    let mode = crate::registry::settings_of(&spec)?.permission_mode;
    plan::start_in(mode, &shared, &mut *shared.stdin.lock().await, &output).await;
    tokio::spawn(supervise(
        bin,
        spec,
        mode,
        options,
        files,
        recovery,
        launched.child,
        launched.stdout,
        launched.stderr,
        shared.clone(),
        output,
    ));
    Ok(Arc::new(ClaudeProcess { shared }))
}
