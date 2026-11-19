use std::path::Path;
use std::process::Stdio;

use protocol::capability::approvals::ApprovalMode;
use protocol::capability::conversation::Start;
use protocol::event::{EventBody, NoticeLevel};
use protocol::model::AgentSessionId;
use protocol::worker::AgentSpec;
use serde_json::{Value, json};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tracing::warn;

use super::files::{ConversationFiles, effective_home};
use super::running::{Shared, effort_str, ensure_sign_in_offers_model, mode_id};
use super::state::TranscriptCursor;
use crate::{AgentError, AgentOptions, acp};

pub(super) const FULL_ACCESS_ARGS: [&str; 3] = ["acp", "--disable-telemetry", "--dangerously-skip-permissions"];
pub(super) const PLAN_ARGS: [&str; 2] = ["acp", "--disable-telemetry"];
const TITLE_ARGS: [&str; 3] = ["acp", "--disable-telemetry", "--include-default-customizations=false"];

#[derive(Clone, Copy, Debug)]
pub(super) enum Purpose {
    Agent,
    Title(&'static str),
}

impl Purpose {
    pub(super) fn args(self, mode: ApprovalMode) -> &'static [&'static str] {
        match self {
            Purpose::Agent if mode == super::claims::full_access() => &FULL_ACCESS_ARGS,
            Purpose::Agent => &PLAN_ARGS,
            Purpose::Title(_) => &TITLE_ARGS,
        }
    }

    pub(super) fn skips_permissions(self, mode: ApprovalMode) -> bool {
        matches!(self, Purpose::Agent) && mode == super::claims::full_access()
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

pub(super) async fn initialize(shared: &Shared) -> Result<Option<String>, AgentError> {
    let reply = shared.rpc().ask("initialize", acp::initialize_params("roundhouse", env!("CARGO_PKG_VERSION"))).await?;
    Ok(acp::parse_agent_version(&reply))
}

pub(super) async fn conversation(
    shared: &Shared,
    spec: &AgentSpec,
    options: &AgentOptions,
    purpose: Purpose,
    agent_version: Option<String>,
) -> Result<(), AgentError> {
    let servers = session_servers(purpose, options);
    let told = session_instructions(purpose, options);
    let home = effective_home(&spec.env, options);
    let response = open_session(shared, spec, &servers, told, home).await?;
    let parsed = acp::parse_session_response(&response)
        .ok_or_else(|| AgentError::Failed("Antigravity returned no session ID".into()))?;
    let id = AgentSessionId::from(parsed.session_id);
    let transcript = match (purpose, home.and_then(|dir| ConversationFiles::of(dir, &id))) {
        (Purpose::Agent, Some(files)) => {
            let path = files.transcript();
            let resumed_same = matches!(&spec.start, Start::Resume { conversation } if *conversation == id);
            let offset = if resumed_same { tokio::fs::metadata(&path).await.map_or(0, |meta| meta.len()) } else { 0 };
            Some(TranscriptCursor::new(path, offset))
        }
        _ => None,
    };
    {
        let mut state = shared.state.lock();
        state.session_id = Some(id.clone());
        state.transcript = transcript;
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
                mode: Some(mode),
                agent_version,
            },
            None,
        )
        .await;
    Ok(())
}

pub(super) async fn resume_after_swap(
    shared: &Shared,
    id: &AgentSessionId,
    mode: ApprovalMode,
) -> Result<(), AgentError> {
    let purpose = shared.context.purpose;
    let options = &shared.context.options;
    let mut params =
        session_params(&shared.context.cwd, &session_servers(purpose, options), session_instructions(purpose, options));
    params["sessionId"] = json!(id);
    let response = shared.rpc().ask("session/resume", params).await?;
    let parsed = acp::parse_session_response(&response)
        .ok_or_else(|| AgentError::Failed("Antigravity returned no session ID".into()))?;
    let existing_transcript = shared.state.lock().transcript.clone();
    let updated_transcript = match existing_transcript {
        Some(prev) => {
            let floor = prev.wakeup_floor.max(prev.async_agents_offset);
            let offset = tokio::fs::metadata(&prev.path).await.map_or(floor, |m| m.len().max(floor));
            Some(TranscriptCursor::new(prev.path, offset))
        }
        None => None,
    };
    {
        let mut state = shared.state.lock();
        state.process_mode = mode;
        state.skip_permissions = purpose.skips_permissions(mode);
        state.transcript = updated_transcript;
        state.config_options = parsed.config_options;
    }
    apply_initial_settings(shared, id, matches!(purpose, Purpose::Agent)).await
}

fn session_servers(purpose: Purpose, options: &AgentOptions) -> Vec<Value> {
    match (purpose, &options.tools) {
        (Purpose::Agent, Some(tools)) => vec![acp::mcp_http_server("roundhouse", tools)],
        _ => Vec::new(),
    }
}

fn session_instructions(purpose: Purpose, options: &AgentOptions) -> Option<&str> {
    match purpose {
        Purpose::Agent => options.instructions.as_ref().map(|told| told.text.as_str()),
        Purpose::Title(instructions) => Some(instructions),
    }
}

fn session_params(cwd: &Path, servers: &[Value], told: Option<&str>) -> Value {
    let mut params = json!({ "cwd": cwd, "mcpServers": servers });
    if let Some(append) = told.filter(|s| !s.is_empty()) {
        params["_meta"] = json!({ "antigravity": { "systemPrompt": { "append": append } } });
    }
    params
}

async fn open_session(
    shared: &Shared,
    spec: &AgentSpec,
    servers: &[Value],
    told: Option<&str>,
    home: Option<&Path>,
) -> Result<Value, AgentError> {
    let rpc = shared.rpc();
    let cwd = Path::new(&spec.cwd);
    match &spec.start {
        Start::New { conversation: None } => rpc.ask("session/new", session_params(cwd, servers, told)).await,
        Start::Resume { conversation } => {
            let mut params = session_params(cwd, servers, told);
            params["sessionId"] = json!(conversation);
            let resumed = rpc.ask("session/resume", params).await;
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
                return rpc.ask("session/new", session_params(cwd, servers, told)).await;
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
    let rpc = shared.rpc();
    let (mode, model, effort) = {
        let state = shared.state.lock();
        (state.process_mode, state.model.clone(), state.effort.clone())
    };
    ensure_sign_in_offers_model(&shared.state.lock().config_options, &model)?;
    rpc.ask("session/set_mode", json!({ "sessionId": id, "modeId": mode_id(mode) })).await?;
    let model_reply = rpc
        .ask("session/set_config_option", json!({ "sessionId": id, "configId": "model", "value": model.as_str() }))
        .await?;
    update_options(shared, &model_reply);
    let level = effort_str(&effort)?;
    let (has_thought, offers_level) = {
        let state = shared.state.lock();
        (
            state.config_options.iter().any(|opt| opt.id == "thought_level"),
            acp::config_offers_value(&state.config_options, "thought_level", level),
        )
    };
    if offers_level {
        let reply = rpc
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
