use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use protocol::attachment::{AttachmentFile, NewAttachment};
use protocol::capability::conversation::Start;
use protocol::capability::settings::{Effort, Settings};
use protocol::model::{AgentAccount, AgentKind, ModelName, RunId, Title};
use protocol::prompt::Prompt;
use protocol::worker::AgentSpec;
use serde_json::json;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::mpsc;

use super::description::TRIAL_MODEL;
use super::files::ConversationFiles;
use super::launch::Purpose;
use super::{claims, control, running};
use crate::stdio::GroupGuard;
use crate::title::{NAMING, read_title};
use crate::{AgentError, AgentOptions, AgentProcess, CallFailed, NoAccount, NoTitle, acp};

const ACCOUNT_WAIT: Duration = Duration::from_secs(15);
const TITLE_WAIT: Duration = Duration::from_secs(90);
const CLOSE_WAIT: Duration = Duration::from_secs(3);

/// Checks login status via `agy models`, which creates no conversation files or summary-DB entries on disk.
pub async fn account(options: &AgentOptions, scratch_dir: &Path) -> Result<AgentAccount, AgentError> {
    if options.login.is_some() {
        return Err(AgentError::Refused("Antigravity uses the login on its worker's machine".into()));
    }
    let bin = running::binary(options)?;
    let (status, stdout, stderr) = run_models(&bin, scratch_dir, options.pid_file.clone()).await?;
    Ok(parse_models_account(status.success(), &stdout, &stderr)?)
}

pub(super) fn parse_models_account(ok: bool, stdout: &str, stderr: &str) -> Result<AgentAccount, NoAccount> {
    if ok {
        let login = if stdout.contains("claude-") || stdout.contains("gemini-pro-orchestrator") {
            Some("oauth".into())
        } else {
            Some("api_key".into())
        };
        return Ok(AgentAccount {
            logged_in: true,
            id: None,
            model_provider: Some("Google".into()),
            plan: None,
            login,
            email: None,
        });
    }
    let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    if combined.contains("sign in") || combined.contains("authentication") || combined.contains("api key") {
        return Ok(AgentAccount {
            logged_in: false,
            id: None,
            model_provider: Some("Google".into()),
            plan: None,
            login: None,
            email: None,
        });
    }
    Err(NoAccount::Unreadable)
}

async fn run_models(
    bin: &Path,
    cwd: &Path,
    pid_file: Option<PathBuf>,
) -> Result<(std::process::ExitStatus, String, String), AgentError> {
    let mut command = Command::new(bin);
    command
        .arg("models")
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    process::own_group(command.as_std_mut());
    let mut child = command.spawn().map_err(|source| AgentError::Start {
        bin: bin.display().to_string(),
        cwd: cwd.display().to_string(),
        source,
    })?;
    let pid = child.id().unwrap_or_default();
    let guard = GroupGuard::new(pid, pid_file);
    let (Some(mut stdout), Some(mut stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return Err(AgentError::NoPipes { bin: bin.display().to_string() });
    };
    let collect = async {
        let mut out_buf = Vec::new();
        let mut err_buf = Vec::new();
        let (out_res, err_res, wait_res) =
            tokio::join!(stdout.read_to_end(&mut out_buf), stderr.read_to_end(&mut err_buf), child.wait());
        out_res.map_err(CallFailed::Io)?;
        err_res.map_err(CallFailed::Io)?;
        let status = wait_res.map_err(CallFailed::Io)?;
        Ok::<_, CallFailed>((
            status,
            String::from_utf8_lossy(&out_buf).into_owned(),
            String::from_utf8_lossy(&err_buf).into_owned(),
        ))
    };
    let result =
        tokio::time::timeout(ACCOUNT_WAIT, collect).await.map_err(|_| CallFailed::TimedOut(ACCOUNT_WAIT))??;
    guard.finished();
    Ok(result)
}

pub async fn title_for(
    prompt: &Prompt<NewAttachment>,
    options: &AgentOptions,
    scratch_dir: &Path,
) -> Result<Title, AgentError> {
    let scratch_dir = scratch_dir.to_owned();
    let room = tokio::task::spawn_blocking(move || tempfile::tempdir_in(scratch_dir))
        .await
        .map_err(|_| AgentError::Exited)?
        .map_err(NoTitle::NoRoom)?;
    let mut files = Vec::new();
    for (i, file) in prompt.files.iter().enumerate() {
        let path = room.path().join(format!("image-{i}"));
        tokio::fs::write(&path, file.data.as_slice()).await?;
        files.push(AttachmentFile { attachment: file.attachment.clone(), path: path.to_string_lossy().into_owned() });
    }
    let spec = title_spec(room.path());
    let (tx, mut rx) = mpsc::channel(128);
    let local_options = AgentOptions { tools: None, ..options.clone() };
    // Starting `agy` counts against the title's time too, since its requests each wait out their own.
    let deadline = tokio::time::Instant::now() + TITLE_WAIT;
    let timed_out = || AgentError::NoTitle(NoTitle::Call(CallFailed::TimedOut(TITLE_WAIT)));
    let bin = running::binary(&local_options)?;
    let started = running::start_conversation(&bin, &spec, tx, &local_options, Purpose::Title(NAMING), None);
    let process = tokio::time::timeout_at(deadline, started).await.map_err(|_| timed_out())??;
    let session_id = process.shared.state.lock().session_id.clone();
    let ask_text = format!("Write a short title for this task description:\n\n{}", prompt.text);
    let ask = async {
        let blocks = tokio::task::spawn_blocking(move || acp::prompt_blocks(&Prompt { text: ask_text, files }))
            .await
            .map_err(|_| AgentError::Exited)?;
        control::send_prompt_rpc(&process.shared, None, blocks).await?;
        read_title(&mut rx).await
    };
    let result = tokio::time::timeout_at(deadline, ask).await.unwrap_or_else(|_| Err(timed_out()));
    drop(rx);
    if result.is_ok()
        && let Some(id) = &session_id
    {
        let _ =
            tokio::time::timeout(CLOSE_WAIT, process.shared.rpc.ask("session/close", json!({ "sessionId": id }))).await;
    }
    process.stop().await;
    if let Some(files) =
        session_id.as_ref().zip(options.binary.home.as_deref()).and_then(|(id, home)| ConversationFiles::of(home, id))
    {
        files.remove().await;
    }
    result
}

fn title_spec(cwd: &Path) -> AgentSpec {
    AgentSpec {
        session_id: "title".into(),
        run: RunId::new(1),
        agent: AgentKind::Antigravity,
        cwd: cwd.to_string_lossy().into_owned(),
        start: Start::New { conversation: None },
        settings: Settings {
            model: Some(ModelName::named(TRIAL_MODEL)),
            mode: Some(claims::plan()),
            effort: Some(Effort::Low),
            ..Settings::default()
        },
        env: Vec::new(),
        tools_token: None,
        login: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_parser_distinguishes_oauth_api_key_and_signed_out() {
        let oauth = parse_models_account(true, "gemini-4-carbon\nclaude-sonnet-4-6\n", "").unwrap();
        assert!(oauth.logged_in);
        assert_eq!(oauth.login.as_deref(), Some("oauth"));

        let api_key = parse_models_account(true, "gemini-4-carbon\ngemini-3.8-flash\n", "").unwrap();
        assert!(api_key.logged_in);
        assert_eq!(api_key.login.as_deref(), Some("api_key"));

        let signed_out = parse_models_account(false, "", "Please sign in to view available models").unwrap();
        assert!(!signed_out.logged_in);
    }
}
