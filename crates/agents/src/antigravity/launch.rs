use std::path::Path;
use std::process::Stdio;

use protocol::capability::conversation::Start;
use protocol::event::{EventBody, NoticeLevel};
use protocol::model::AgentSessionId;
use protocol::worker::AgentSpec;
use serde_json::{Value, json};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tracing::warn;

use super::files::ConversationFiles;
use super::running::{Shared, effort_str, mode_id};
use crate::{AgentError, AgentOptions, acp};

pub(super) const ARGS: [&str; 2] = ["acp", "--disable-telemetry"];
const TITLE_ARGS: [&str; 3] = ["acp", "--disable-telemetry", "--include-default-customizations=false"];

#[derive(Clone, Copy, Debug)]
pub(super) enum Purpose {
    Agent,
    Title(&'static str),
}

impl Purpose {
    pub(super) fn args(self) -> &'static [&'static str] {
        match self {
            Purpose::Agent => &ARGS,
            Purpose::Title(_) => &TITLE_ARGS,
        }
    }
}

pub(super) struct Launched {
    pub(super) child: Child,
    pub(super) pid: u32,
    pub(super) stdin: ChildStdin,
    pub(super) stdout: ChildStdout,
    pub(super) stderr: ChildStderr,
}

pub(super) fn launch(
    bin: &Path,
    args: &[&str],
    cwd: &Path,
    env: &[(String, String)],
    options: &AgentOptions,
) -> Result<Launched, AgentError> {
    if options.login.is_some() {
        return Err(AgentError::Refused("Antigravity uses the login on its worker's machine".into()));
    }
    let mut command = Command::new(bin);
    command
        .args(args)
        .current_dir(cwd)
        .envs(env.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    process::own_group(command.as_std_mut());
    let mut child = crate::starting::start(&mut command, bin, cwd, &options.environment)?;
    let pid = child.id().unwrap_or_default();
    if let Some(path) = &options.pid_file
        && let Err(error) = process::ProcessRecord::write(path, pid)
    {
        warn!("could not record Antigravity {pid} in {}: {error}", path.display());
    }
    let (Some(stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err(AgentError::NoPipes { bin: bin.display().to_string() });
    };
    Ok(Launched { child, pid, stdin, stdout, stderr })
}

pub(super) async fn initialize(shared: &Shared) -> Result<(), AgentError> {
    shared.rpc.ask("initialize", acp::initialize_params("roundhouse", env!("CARGO_PKG_VERSION"))).await?;
    Ok(())
}

pub(super) async fn conversation(
    shared: &Shared,
    spec: &AgentSpec,
    options: &AgentOptions,
    purpose: Purpose,
) -> Result<(), AgentError> {
    let servers = match (purpose, &options.tools) {
        (Purpose::Agent, Some(tools)) => vec![acp::mcp_http_server("roundhouse", tools)],
        _ => Vec::new(),
    };
    let told = match purpose {
        Purpose::Agent => options.instructions.as_ref().map(|told| told.text.as_str()),
        Purpose::Title(instructions) => Some(instructions),
    };
    let response = open_session(shared, spec, &servers, told, options.binary.home.as_deref()).await?;
    let parsed = acp::parse_session_response(&response)
        .ok_or_else(|| AgentError::Failed("Antigravity returned no session ID".into()))?;
    let id = AgentSessionId::from(parsed.session_id);
    {
        let mut state = shared.state.lock();
        state.session_id = Some(id.clone());
        state.config_options = parsed.config_options;
    }
    apply_initial_settings(shared, &id, matches!(purpose, Purpose::Agent)).await?;
    let (model, mode) = {
        let state = shared.state.lock();
        (state.model.as_str().to_owned(), state.mode)
    };
    shared
        .event(
            EventBody::AgentInit {
                capabilities: None,
                agent_session_id: Some(id),
                model: Some(model),
                cwd: Some(spec.cwd.clone()),
                permission_mode: Some(mode),
                agent_version: None,
            },
            None,
        )
        .await;
    Ok(())
}

async fn open_session(
    shared: &Shared,
    spec: &AgentSpec,
    servers: &[Value],
    told: Option<&str>,
    home: Option<&Path>,
) -> Result<Value, AgentError> {
    let new_params = || {
        let mut params = json!({ "cwd": spec.cwd, "mcpServers": servers });
        if let Some(append) = told.filter(|s| !s.is_empty()) {
            params["_meta"] = json!({ "antigravity": { "systemPrompt": { "append": append } } });
        }
        params
    };
    match &spec.start {
        Start::New { conversation: None } => shared.rpc.ask("session/new", new_params()).await,
        Start::Resume { conversation } => {
            let mut params = new_params();
            params["sessionId"] = json!(conversation);
            let resumed = shared.rpc.ask("session/resume", params).await;
            if let Err(error) = &resumed
                && conversation_gone(error, conversation, home).await
            {
                shared
                    .event(
                        EventBody::Notice {
                            level: NoticeLevel::Warning,
                            message: "Antigravity could not find this conversation. Starting a new one without its \
                                previous context."
                                .into(),
                            subagent: None,
                        },
                        None,
                    )
                    .await;
                return shared.rpc.ask("session/new", new_params()).await;
            }
            resumed
        }
        Start::New { conversation: Some(_) } | Start::Fork { .. } | Start::Import { .. } => {
            Err(AgentError::Refused("Antigravity cannot start that conversation".into()))
        }
    }
}

/// Whether `agy` refused to resume `conversation` because it no longer has it. `agy` 1.5.9 answers -32603, "Failed to
/// read session <id>; retry, or load it with session/load", whether the conversation's file is gone, unreadable or
/// corrupt, so only its file being gone says the conversation is. Without the machine's home to look in, nothing does,
/// and the resume fails rather than lose a conversation that may still be there.
async fn conversation_gone(error: &AgentError, conversation: &AgentSessionId, home: Option<&Path>) -> bool {
    let AgentError::Failed(message) = error else { return false };
    if !message.starts_with("Failed to read session") {
        return false;
    }
    match home.and_then(|home| ConversationFiles::of(home, conversation)) {
        Some(files) => files.gone().await,
        None => false,
    }
}

async fn apply_initial_settings(shared: &Shared, id: &AgentSessionId, strict_effort: bool) -> Result<(), AgentError> {
    let (mode, model, effort) = {
        let state = shared.state.lock();
        (state.mode, state.model.clone(), state.effort)
    };
    shared.rpc.ask("session/set_mode", json!({ "sessionId": id, "modeId": mode_id(mode) })).await?;
    let model_reply = shared
        .rpc
        .ask("session/set_config_option", json!({ "sessionId": id, "configId": "model", "value": model.as_str() }))
        .await?;
    update_options(shared, &model_reply);
    let level = effort_str(effort)?;
    let (has_thought, offers_level) = {
        let state = shared.state.lock();
        (
            state.config_options.iter().any(|opt| opt.id == "thought_level"),
            acp::config_offers_value(&state.config_options, "thought_level", level),
        )
    };
    if offers_level {
        let reply = shared
            .rpc
            .ask("session/set_config_option", json!({ "sessionId": id, "configId": "thought_level", "value": level }))
            .await?;
        update_options(shared, &reply);
    } else if strict_effort && has_thought {
        return Err(AgentError::Refused("This Antigravity model does not offer that effort".into()));
    }
    Ok(())
}

pub(super) fn update_options(shared: &Shared, reply: &Value) {
    if let Some(options) =
        reply.get("configOptions").and_then(|v| serde_json::from_value::<Vec<acp::ConfigOption>>(v.clone()).ok())
    {
        shared.state.lock().config_options = options;
    }
}
