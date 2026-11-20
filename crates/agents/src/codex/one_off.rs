use std::path::{Path, PathBuf};
use std::time::Duration;

use protocol::attachment::{AttachmentFile, NewAttachment};
use protocol::capability::conversation::Start;
use protocol::capability::settings::Settings;
use protocol::model::{AgentAccount, AgentKind, ModelName, RunId, Title};
use protocol::prompt::Prompt;
use protocol::worker::AgentSpec;
use serde_json::json;
use tokio::sync::mpsc;

use super::launch::Purpose;
use super::running;
use crate::title::{NAMING, read_title};
use crate::{AgentError, AgentOptions, AgentProcess, NoTitle};

/// What a one-off call runs, which starts no chat, so it's none a chat is offered: a small model, at a low effort.
pub(super) const ONE_OFF_MODEL: ModelName = ModelName::named("gpt-6-luna");

pub async fn account(options: &AgentOptions, scratch_dir: &Path) -> Result<AgentAccount, AgentError> {
    let process = open(options, scratch_dir).await?;
    let response = process.shared.rpc.ask("account/read", json!({"refreshToken":false})).await;
    process.stop().await;
    let response = response?;
    let account = &response["account"];
    let email = account["email"].as_str().map(str::to_owned);
    Ok(AgentAccount {
        logged_in: account.is_object(),
        id: email.as_ref().map(|email| format!("codex:{email}").into()),
        model_provider: Some("OpenAI".into()),
        plan: account["planType"].as_str().map(str::to_owned),
        login: account["type"].as_str().map(str::to_owned),
        email,
    })
}

/// Initialize app-server without creating or resuming a thread. Unsolicited output has no session to receive it.
pub(super) async fn open(
    options: &AgentOptions,
    cwd: &Path,
) -> Result<std::sync::Arc<running::CodexProcess>, AgentError> {
    let (tx, rx) = mpsc::channel(1);
    drop(rx);
    let settings = crate::registry::on(AgentKind::Codex, ONE_OFF_MODEL)?;
    running::transport(&binary(options)?, cwd, &[], settings, tx, options, None).await
}

/// A read-only conversation in an empty directory, without persistent history or worker tools.
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
    let spec = AgentSpec {
        session_id: "title".into(),
        run: RunId::new(1),
        agent: AgentKind::Codex,
        cwd: room.path().to_string_lossy().into_owned(),
        start: Start::New { conversation: None },
        settings: Settings { model: Some(ONE_OFF_MODEL), ..Settings::default() },
        env: Vec::new(),
        tools_token: None,
        login: None,
        repository: None,
    };
    let (tx, mut rx) = mpsc::channel(128);
    let options = AgentOptions { tools: None, ..options.clone() };
    let process =
        running::start_conversation(&binary(&options)?, &spec, tx, &options, Purpose::Title(NAMING), None).await?;
    let response = async {
        let thread = process.shared.state.lock().thread.clone().ok_or(AgentError::Exited)?;
        let mut input = vec![
            json!({"type":"text","text":format!("Write a short title for this task description:\n\n{}", prompt.text),"text_elements":[]}),
        ];
        input.extend(files.iter().map(|file| json!({"type":"localImage","path":file.path})));
        process
            .shared
            .rpc
            .ask(
                "turn/start",
                json!({"threadId":thread,"input":input,
            "model":ONE_OFF_MODEL.as_str(),"effort":"low","approvalPolicy":"never",
            "sandboxPolicy":{"type":"readOnly"}}),
            )
            .await?;
        read_title(&mut rx).await
    };
    let result = tokio::time::timeout(Duration::from_secs(90), response).await.unwrap_or(Err(AgentError::NoAnswer));
    drop(rx);
    process.stop().await;
    result
}

fn binary(options: &AgentOptions) -> Result<PathBuf, AgentError> {
    crate::binary::resolve_binary("codex", &[], &options.binary)
        .ok_or_else(|| AgentError::Refused("Could not find the Codex CLI".into()))
}
