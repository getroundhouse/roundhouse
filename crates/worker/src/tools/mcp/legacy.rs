//! MCP 2025-06-18, used by Codex. Requests have no 2026 method/name headers or per-request capability metadata.
//! This version has no tasks extension, so it exposes only tools answered immediately, sharing their hub asks.

use super::super::{diff_comments, proposals, pull_request};
use super::wire::{Code, Headers, RpcError, Said, agrees, unsupported};
use crate::asks::Asks;
use protocol::{Build, model::SessionId};
use serde_json::{Map, Value, json};

pub(super) const VERSION: &str = "2025-06-18";

// This envelope offers the workspace-only lookup, rather than the current envelope's explicit PR selections and waits.
const INSTRUCTIONS: &str = "Roundhouse's tools, for the pull request of the branch checked out in this workspace, for \
    the comments the person has left on this workspace's diff, and for proposing a decision too large for a question, \
    which the person answers in a tab of its own. For anything about the pull request, look among these before you \
    use `gh` or write a loop of your own.";

pub(super) async fn request(
    asks: &Asks,
    build: Build,
    session_id: &SessionId,
    headers: &Headers,
    method: &str,
    params: &Map<String, Value>,
) -> Result<Value, RpcError> {
    if method == "initialize" {
        let requested = params.get("protocolVersion").and_then(Value::as_str);
        if headers.version != Said::Nothing {
            agrees("MCP-Protocol-Version", &headers.version, requested)?;
        }
        if requested != Some(VERSION) {
            return Err(unsupported(requested));
        }
        return Ok(json!({"protocolVersion":VERSION,"capabilities":{"tools":{}},
            "serverInfo":{"name":"roundhouse","version":build.version()},"instructions":INSTRUCTIONS}));
    }
    agrees("MCP-Protocol-Version", &headers.version, Some(VERSION))?;
    match method {
        "ping" => Ok(json!({})),
        "tools/list" => {
            let mut tools = vec![pull_request::definition(), diff_comments::definition()];
            tools.extend(proposals::definitions());
            Ok(json!({ "tools": tools }))
        }
        "tools/call" => {
            let called = match params.get("name").and_then(Value::as_str) {
                Some(pull_request::NAME) => pull_request::call(asks, session_id).await,
                Some(diff_comments::NAME) => diff_comments::call(asks, session_id).await,
                Some(proposals::PROPOSE) => proposals::propose(asks, session_id, params).await,
                Some(proposals::GET) => proposals::get(asks, session_id, params).await,
                _ => {
                    let offered = [pull_request::NAME, diff_comments::NAME, proposals::PROPOSE, proposals::GET];
                    let offered = format!("this protocol offers only {}", offered.join(", "));
                    return Err(RpcError::new(Code::InvalidParams, offered));
                }
            };
            Ok(json!({"content":[{"type":"text","text":called.text}],"isError":called.is_error}))
        }
        _ => Err(RpcError::new(Code::MethodNotFound, format!("there's no method {method} here"))),
    }
}
