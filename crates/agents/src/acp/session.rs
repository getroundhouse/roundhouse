use protocol::attachment::AttachmentFile;
use protocol::prompt::Prompt;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::ToolServer;

const PROTOCOL_VERSION: u32 = 1;

/// Client capabilities telling an ACP server to execute its own filesystem and terminal tools while routing structured
/// multiple-choice and free-text questions over `elicitation/create`.
pub(crate) fn initialize_params(client_name: &str, client_version: &str) -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "clientInfo": { "name": client_name, "version": client_version },
        "clientCapabilities": {
            "fs": { "readTextFile": false, "writeTextFile": false },
            "terminal": false,
            "elicitation": { "form": true }
        }
    })
}

/// An HTTP MCP server entry for ACP `session/new` and `session/resume`, keeping the bearer token off the command line
/// and off disk.
pub(crate) fn mcp_http_server(name: &str, tools: &ToolServer) -> Value {
    json!({
        "type": "http",
        "name": name,
        "url": tools.url,
        "headers": [{ "name": "Authorization", "value": format!("Bearer {}", tools.token.as_str()) }]
    })
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InitializeResponse {
    #[serde(default)]
    agent_info: Option<AgentInfo>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AgentInfo {
    #[serde(default)]
    version: Option<String>,
}

/// Reads `agentInfo.version` from an ACP `initialize` response when present and non-empty.
pub(crate) fn parse_agent_version(value: &Value) -> Option<String> {
    serde_json::from_value::<InitializeResponse>(value.clone())
        .ok()
        .and_then(|response| response.agent_info)
        .and_then(|info| info.version)
        .filter(|version| !version.is_empty())
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionResponse {
    #[serde(default)]
    pub(crate) session_id: String,
    #[serde(default)]
    pub(crate) config_options: Vec<ConfigOption>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigOption {
    #[serde(default)]
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) category: Option<String>,
    #[serde(default)]
    pub(crate) current_value: Option<String>,
    #[serde(default)]
    pub(crate) options: Vec<ConfigChoice>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigChoice {
    #[serde(default)]
    pub(crate) value: String,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) description: Option<String>,
}

pub(crate) fn parse_session_response(value: &Value) -> Option<SessionResponse> {
    serde_json::from_value::<SessionResponse>(value.clone()).ok().filter(|r| !r.session_id.is_empty())
}

/// Whether `options` exposes a `ConfigOption` with `config_id` whose choices include `target`.
pub(crate) fn config_offers_value(options: &[ConfigOption], config_id: &str, target: &str) -> bool {
    options
        .iter()
        .find(|option| option.id == config_id)
        .is_some_and(|option| option.options.iter().any(|choice| choice.value == target))
}

/// Builds ACP `session/prompt` content blocks, reading small text and image attachments off async threads when needed.
pub(crate) fn prompt_blocks(prompt: &Prompt<AttachmentFile>) -> Vec<Value> {
    let mut content = crate::attachments::file_content_blocks(prompt, |media_type, data| {
        json!({
            "type": "image",
            "mimeType": media_type.mime(),
            "data": data.to_base64(),
        })
    });
    if prompt.says_anything() || content.is_empty() {
        content.push(json!({ "type": "text", "text": prompt.text }));
    }
    content
}
