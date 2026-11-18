use std::path::Path;
use std::process::Stdio;

use protocol::capability::conversation::Start;
use protocol::event::EventBody;
use protocol::worker::AgentSpec;
use serde_json::{Value, json};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tracing::warn;

use super::running::Shared;
use super::state::ThreadId;
use crate::registry::Launch;
use crate::{AgentError, AgentOptions};

pub(super) const ARGS: [&str; 10] = [
    "app-server",
    "--stdio",
    "-c",
    "features.memories=false",
    "-c",
    "features.goals=false",
    "-c",
    "features.default_mode_request_user_input=true",
    // Codex warns of an under-development feature, such as the one above, in every thread. It's Roundhouse's choice,
    // so the warning tells the person nothing they can act on.
    "-c",
    "suppress_unstable_features_warning=true",
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
    let mut child = crate::starting::start(&mut command, bin, cwd, &options.environment)?;
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

/// What a thread is started for, which decides whether it's kept and what it's told.
#[derive(Clone, Copy, Debug)]
pub(super) enum Purpose {
    /// An agent's work, which is kept, so it can be resumed, and told what every agent is told.
    Agent,
    /// Naming a task, in a thread that isn't kept, told how in place of Codex's own instructions.
    Title(&'static str),
}

pub(super) async fn conversation(
    shared: &Shared,
    spec: &AgentSpec,
    options: &AgentOptions,
    purpose: Purpose,
) -> Result<(), AgentError> {
    let settings = shared.state.lock().settings.clone();
    let told = match purpose {
        Purpose::Agent => options.instructions.as_ref().map(|told| told.text.as_str()),
        Purpose::Title(instructions) => Some(instructions),
    };
    let mut params = configuration(spec, &settings, options, told);
    let response = match &spec.start {
        Start::New { conversation: None } => {
            params["ephemeral"] = json!(matches!(purpose, Purpose::Title(_)));
            if let Purpose::Title(instructions) = purpose {
                title_configuration(shared, &mut params, instructions).await?;
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
                        message: "Codex could not find this conversation. Starting a new one without its previous context.".into(), subagent: None }, None).await;
                    shared.rpc.ask("thread/start", configuration(spec, &settings, options, told)).await?
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
                // The worker says what the run claims as it starts it.
                capabilities: None,
                agent_session_id: Some(id.into()),
                model: response["model"].as_str().map(str::to_owned),
                cwd: Some(spec.cwd.clone()),
                mode: Some(settings.mode),
                agent_version: None,
            },
            None,
        )
        .await;
    Ok(())
}

/// A thread's parameters, told `instructions` as its developer's.
fn configuration(spec: &AgentSpec, settings: &Launch, options: &AgentOptions, instructions: Option<&str>) -> Value {
    let plan = settings.mode.plan;
    let mut config = json!({"features.memories":false,"features.goals":false,"features.default_mode_request_user_input":true,
        "model_reasoning_summary":"auto"});
    if let Some(level) = settings.effort.as_str() {
        config["model_reasoning_effort"] = json!(level);
    }
    if let Some(tools) = &options.tools {
        config["mcp_servers.roundhouse"] = json!({"url":tools.url,
            "http_headers":{"Authorization":format!("Bearer {}", tools.token.as_str())}});
    }
    json!({"cwd":spec.cwd,"model":settings.model.as_str(),
        "approvalPolicy":if plan { "on-request" } else { "never" },
        "sandbox":if plan { "read-only" } else { "danger-full-access" },
        "developerInstructions":instructions,"config":config,"experimentalRawEvents":false})
}

/// Native config overrides merge tables, so an empty `mcp_servers` table leaves the user's servers enabled. Disable
/// each named server and plugin loading for title generation without changing the person's configuration on disk.
async fn title_configuration(shared: &Shared, params: &mut Value, instructions: &str) -> Result<(), AgentError> {
    let native = shared.rpc.ask("config/read", json!({"cwd":params["cwd"],"includeLayers":false})).await?;
    let mut servers = serde_json::Map::new();
    if let Some(configured) = native["config"]["mcp_servers"].as_object() {
        for name in configured.keys() {
            servers.insert(name.clone(), json!({"enabled":false}));
        }
    }
    params["baseInstructions"] = json!(instructions);
    params["approvalPolicy"] = json!("never");
    params["sandbox"] = json!("read-only");
    params["config"]["mcp_servers"] = Value::Object(servers);
    params["config"]["features.plugins"] = json!(false);
    params["config"]["features.hooks"] = json!(false);
    params["config"]["features.apps"] = json!(false);
    // Nothing answers a title's questions, and it has no use for subagents, a browser, the computer or making images.
    // Codex offers its model the same tools with these off, and refuses a call to one that's off, so they keep a title
    // from asking where Codex honours them, and `read_title` ends one that asks anyway.
    params["config"]["features.default_mode_request_user_input"] = json!(false);
    params["config"]["features.multi_agent"] = json!(false);
    params["config"]["features.browser_use"] = json!(false);
    params["config"]["features.in_app_browser"] = json!(false);
    params["config"]["features.computer_use"] = json!(false);
    params["config"]["features.image_generation"] = json!(false);
    params["config"]["web_search"] = json!("disabled");
    params["config"]["project_doc_max_bytes"] = json!(0);
    params["config"]["model_reasoning_summary"] = json!("none");
    Ok(())
}
