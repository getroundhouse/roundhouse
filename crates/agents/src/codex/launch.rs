use std::path::Path;
use std::process::Stdio;

use protocol::capability::conversation::Start;
use protocol::event::EventBody;
use protocol::model::PermissionMode;
use protocol::worker::AgentSpec;
use serde_json::{Value, json};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tracing::warn;

use super::running::Shared;
use super::state::ThreadId;
use crate::{AgentError, AgentOptions};

pub(super) const ARGS: [&str; 8] = [
    "app-server",
    "--stdio",
    "-c",
    "features.memories=false",
    "-c",
    "features.goals=false",
    "-c",
    "features.default_mode_request_user_input=true",
];

pub(super) struct Launched {
    pub(super) child: Child,
    pub(super) pid: u32,
    pub(super) stdin: ChildStdin,
    pub(super) stdout: ChildStdout,
    pub(super) stderr: ChildStderr,
}

pub(super) fn launch(
    bin: &Path,
    cwd: &Path,
    env: &[(String, String)],
    options: &AgentOptions,
) -> Result<Launched, AgentError> {
    if options.login.is_some() {
        return Err(AgentError::Refused("Codex uses the login on its worker's machine".into()));
    }
    let mut command = Command::new(bin);
    command
        .args(ARGS)
        .current_dir(cwd)
        .envs(env.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::piped())
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
    if let Some(path) = &options.pid_file
        && let Err(error) = process::ProcessRecord::write(path, pid)
    {
        warn!("could not record Codex {pid} in {}: {error}", path.display());
    }
    let (Some(stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return Err(AgentError::NoPipes { bin: bin.display().to_string() });
    };
    Ok(Launched { child, pid, stdin, stdout, stderr })
}

pub(super) async fn initialize(shared: &Shared) -> Result<(), AgentError> {
    shared
        .rpc
        .ask(
            "initialize",
            json!({
                "clientInfo":{"name":"roundhouse","version":env!("CARGO_PKG_VERSION")},
                "capabilities":{"experimentalApi":true}
            }),
        )
        .await?;
    shared.rpc.write(&json!({"method":"initialized"})).await
}

pub(super) async fn conversation(
    shared: &Shared,
    spec: &AgentSpec,
    options: &AgentOptions,
    ephemeral: bool,
) -> Result<(), AgentError> {
    let mut params = configuration(spec, options);
    let response = match &spec.start {
        Start::New { conversation: None } => {
            params["ephemeral"] = json!(ephemeral);
            if ephemeral {
                title_configuration(shared, &mut params, options).await?;
            }
            shared.rpc.ask("thread/start", params).await?
        }
        Start::Resume { conversation } => {
            params["threadId"] = json!(conversation);
            params["excludeTurns"] = json!(true);
            match shared.rpc.ask("thread/resume", params).await {
                Ok(response) => response,
                Err(AgentError::Refused(message))
                    if message.contains("no rollout found") || message.contains("thread not found") =>
                {
                    shared.event(EventBody::Notice { level: protocol::event::NoticeLevel::Warning,
                        message: "Codex could not find this conversation. Starting a new one without its previous context.".into(), parent_tool_use_id: None }, None).await;
                    shared.rpc.ask("thread/start", configuration(spec, options)).await?
                }
                Err(error) => return Err(error),
            }
        }
        Start::New { conversation: Some(_) } | Start::Fork { .. } | Start::Import { .. } => {
            return Err(AgentError::Refused("Codex cannot start that conversation".into()));
        }
    };
    let id =
        response["thread"]["id"].as_str().ok_or_else(|| AgentError::Refused("Codex returned no thread ID".into()))?;
    shared.state.lock().thread = Some(ThreadId(id.into()));
    shared
        .event(
            EventBody::AgentInit {
                capabilities: Some(Box::new(super::claims::claims())),
                agent_session_id: Some(id.into()),
                model: response["model"].as_str().map(str::to_owned),
                cwd: Some(spec.cwd.clone()),
                permission_mode: Some(spec.settings.permission_mode),
                agent_version: None,
            },
            None,
        )
        .await;
    Ok(())
}

fn configuration(spec: &AgentSpec, options: &AgentOptions) -> Value {
    let plan = spec.settings.permission_mode == PermissionMode::Plan;
    let mut config = json!({"features.memories":false,"features.goals":false,"features.default_mode_request_user_input":true,
        "model_reasoning_effort":spec.settings.effort.as_str()});
    if let Some(tools) = &options.tools {
        config["mcp_servers.roundhouse"] = json!({"url":tools.url,
            "http_headers":{"Authorization":format!("Bearer {}", tools.token.as_str())}});
    }
    json!({"cwd":spec.cwd,"model":spec.settings.model.as_str(),
        "approvalPolicy":if plan { "on-request" } else { "never" },
        "sandbox":if plan { "read-only" } else { "danger-full-access" },
        "developerInstructions":options.instructions,"config":config,"experimentalRawEvents":false})
}

/// Native config overrides merge tables, so an empty `mcp_servers` table leaves the user's servers enabled. Disable
/// each named server and plugin loading for title generation without changing the person's configuration on disk.
async fn title_configuration(shared: &Shared, params: &mut Value, options: &AgentOptions) -> Result<(), AgentError> {
    let native = shared.rpc.ask("config/read", json!({"cwd":params["cwd"],"includeLayers":false})).await?;
    let mut servers = serde_json::Map::new();
    if let Some(configured) = native["config"]["mcp_servers"].as_object() {
        for name in configured.keys() {
            servers.insert(name.clone(), json!({"enabled":false}));
        }
    }
    params["baseInstructions"] = json!(options.instructions);
    params["approvalPolicy"] = json!("never");
    params["sandbox"] = json!("read-only");
    params["config"]["mcp_servers"] = Value::Object(servers);
    params["config"]["features.plugins"] = json!(false);
    params["config"]["features.hooks"] = json!(false);
    params["config"]["features.apps"] = json!(false);
    params["config"]["web_search"] = json!("disabled");
    params["config"]["project_doc_max_bytes"] = json!(0);
    Ok(())
}
