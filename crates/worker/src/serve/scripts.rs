//! Running a workspace's setup and archive scripts.

use std::path::Path;
use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode};
use protocol::worker::WorkerResponse;

use super::State;
use crate::setup;

/// Signals the script's whole group, because stopping only the shell would leave what it started.
pub(crate) fn stop_setup_script(state: &State) {
    if let Some(pid) = *state.setup_pid.lock() {
        // Its entry is cleared as soon as it has been waited on, so this can't reach a reused PID.
        process::signal_group(pid, process::Signal::Terminate);
    }
}

pub(crate) async fn run_setup(
    state: &Arc<State>,
    repo: &str,
    path: &str,
    env: &[(String, String)],
) -> Result<WorkerResponse, ApiError> {
    let (repo, worktree) = (Path::new(repo), Path::new(path));
    let log = setup::setup_log_path(state.config.state_dir.as_deref(), worktree);
    let _ = std::fs::remove_file(&log);
    let mut warnings = Vec::new();
    let settings = setup::load_settings(repo, worktree, state.config.home.as_deref(), &mut warnings);
    let copied = setup::copy_local_files(repo, worktree, &settings, &mut warnings).await;
    for file in &copied {
        setup::append_log(&log, &format!("Copied {file}"));
    }
    for warning in &warnings {
        setup::append_log(&log, &format!("Warning: {warning}"));
    }
    let Some(script) = settings.setup else {
        return Ok(WorkerResponse::SetupDone { code: Some(0) });
    };
    setup::append_log(&log, "Running the setup script");
    // Recorded like an agent, so the provider can stop it if this worker is killed.
    let record = state.config.state_dir.as_ref().map(|dir| dir.join("agents/setup.pid"));
    let (setup_pid, for_start) = (state.setup_pid.clone(), record.clone());
    let status = setup::run_script(&script, worktree, env, &log, move |pid| {
        *setup_pid.lock() = Some(pid);
        if let Some(path) = &for_start {
            let _ = process::ProcessRecord::write(path, pid);
        }
    })
    .await;
    *state.setup_pid.lock() = None;
    if let Some(path) = &record {
        let _ = std::fs::remove_file(path);
    }
    let code = status?.code();
    match code {
        Some(code) => setup::append_log(&log, &format!("The setup script exited with {code}")),
        None => setup::append_log(&log, "The setup script was stopped"),
    }
    Ok(WorkerResponse::SetupDone { code })
}

pub(crate) async fn run_archive_script(
    state: &Arc<State>,
    repo: &str,
    path: &str,
    env: &[(String, String)],
) -> Result<WorkerResponse, ApiError> {
    let (repo, worktree) = (Path::new(repo), Path::new(path));
    if !worktree.exists() {
        return Ok(WorkerResponse::Ok);
    }
    let settings = setup::load_settings(repo, worktree, state.config.home.as_deref(), &mut Vec::new());
    let Some(script) = settings.archive else { return Ok(WorkerResponse::Ok) };
    let log = setup::setup_log_path(state.config.state_dir.as_deref(), worktree);
    setup::append_log(&log, "Running the archive script");
    let status = setup::run_script(&script, worktree, env, &log, |_| {}).await?;
    if status.success() {
        return Ok(WorkerResponse::Ok);
    }
    Err(ApiError::new(
        ErrorCode::NeedsForce,
        format!("the archive script failed with {:?}. Fix it, or archive with force", status.code()),
    ))
}
