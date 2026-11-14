use super::super::{pull_request, pull_requests, wait};
use crate::asks::Asks;
use protocol::Build;
use protocol::model::SessionId;
use serde_json::{Map, Value, json};

/// The MCP version with stateless discovery and tasks.
pub(in super::super) const VERSION: &str = "2026-07-28";

/// Where a request's `_meta` names its version.
pub(super) const VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";

/// Where a request's `_meta` says what its client can do.
pub(super) const CAPABILITIES_KEY: &str = "io.modelcontextprotocol/clientCapabilities";

/// MCP's tasks extension, which a client offers in each request it can take a task for.
pub(super) const TASKS: &str = "io.modelcontextprotocol/tasks";

/// What the agent is told of these tools from its first turn: what they're for, as a whole, and what to look among
/// them for, since the Claude CLI shows it each tool only by name until it loads that tool, as `docs/claude-cli.md`
/// says. When to use each, and how, is in its description, so this needn't grow with each tool, as "The worker's
/// tools" in `docs/architecture.md` says.
const INSTRUCTIONS: &str = "Roundhouse's tools, for pull requests in this workspace's repository and their lower stack \
    dependencies. For reading their current state or waiting in the background until checks pass, they merge or \
    need attention, look among these before you use `gh` or write a loop of your own.";

/// What one of a request's HTTP headers says, which has to agree with its body.
#[derive(Debug, PartialEq, Eq)]
pub(in super::super) enum Said {
    Nothing,
    /// Not text, or not text in the encoding its header allows.
    Unreadable,
    Value(String),
}

/// What a request's HTTP headers say of it.
#[derive(Debug)]
pub(in super::super) struct Headers {
    pub(in super::super) version: Said,
    pub(in super::super) method: Said,
    /// The tool a call names.
    pub(in super::super) name: Said,
}

pub(in super::super) enum Reply {
    /// A notification, which nothing answers.
    Accepted,
    Answer(Answer),
}

pub(in super::super) struct Answer {
    /// The request's own, or `null` when it couldn't be read.
    pub(super) id: Value,
    pub(in super::super) outcome: Result<Value, RpcError>,
}

impl Answer {
    pub(in super::super) fn to_json(&self) -> Value {
        match &self.outcome {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": self.id, "result": result }),
            Err(error) => {
                let mut body = json!({ "code": error.code.number(), "message": error.message });
                if let Some(data) = &error.data {
                    body["data"] = data.clone();
                }
                json!({ "jsonrpc": "2.0", "id": self.id, "error": body })
            }
        }
    }
}

/// JSON-RPC's codes, and MCP's, that these tools answer with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum Code {
    Parse,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    Internal,
    HeaderMismatch,
    MissingCapability,
    UnsupportedVersion,
}

impl Code {
    pub(super) fn number(self) -> i32 {
        match self {
            Code::Parse => -32700,
            Code::InvalidRequest => -32600,
            Code::MethodNotFound => -32601,
            Code::InvalidParams => -32602,
            Code::Internal => -32603,
            Code::HeaderMismatch => -32020,
            Code::MissingCapability => -32021,
            Code::UnsupportedVersion => -32022,
        }
    }
}

#[derive(Debug)]
pub(in super::super) struct RpcError {
    pub(in super::super) code: Code,
    pub(super) message: String,
    pub(super) data: Option<Value>,
}

impl RpcError {
    pub(in super::super) fn new(code: Code, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), data: None }
    }

    /// For a client that didn't offer to take a task, where only a task will do.
    pub(super) fn needs_tasks() -> Self {
        Self {
            code: Code::MissingCapability,
            message: "Missing required client capability: this needs MCP's tasks extension".into(),
            data: Some(json!({ "requiredCapabilities": { "extensions": { TASKS: {} } } })),
        }
    }
}

/// The methods these tools answer, and the one of the versions before, which they refuse with the version they speak.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Method<'a> {
    Discover,
    Ping,
    ListTools,
    CallTool,
    GetTask,
    UpdateTask,
    CancelTask,
    Initialize,
    Other(&'a str),
}

impl<'a> Method<'a> {
    pub(super) fn named(name: &'a str) -> Self {
        match name {
            "server/discover" => Method::Discover,
            "ping" => Method::Ping,
            "tools/list" => Method::ListTools,
            "tools/call" => Method::CallTool,
            "tasks/get" => Method::GetTask,
            "tasks/update" => Method::UpdateTask,
            "tasks/cancel" => Method::CancelTask,
            "initialize" => Method::Initialize,
            other => Method::Other(other),
        }
    }
}

/// Answers one message, from the agent in `session_id`. Only a JSON-RPC request or notification is taken, since a
/// client sends nothing else.
pub(in super::super) async fn answer(
    asks: &Asks,
    build: Build,
    session_id: &SessionId,
    headers: &Headers,
    body: &[u8],
) -> Reply {
    let message: Value = match serde_json::from_slice(body) {
        Ok(message) => message,
        Err(e) => return refused(Value::Null, Code::Parse, format!("that isn't JSON: {e}")),
    };
    let Some(message) = message.as_object() else {
        return refused(Value::Null, Code::InvalidRequest, "a message is an object");
    };
    // A request's ID is a string or a number, and a notification has none.
    let id = match message.get("id") {
        None => None,
        Some(id @ (Value::String(_) | Value::Number(_))) => Some(id.clone()),
        Some(_) => return refused(Value::Null, Code::InvalidRequest, "a request's ID is a string or a number"),
    };
    let method = message.get("method").and_then(Value::as_str);
    let (Some(method), Some("2.0")) = (method, message.get("jsonrpc").and_then(Value::as_str)) else {
        return refused(id.unwrap_or(Value::Null), Code::InvalidRequest, "a JSON-RPC 2.0 request names its method");
    };
    let Some(id) = id else { return Reply::Accepted };
    let params = message.get("params").and_then(Value::as_object).cloned().unwrap_or_default();
    Reply::Answer(Answer { id, outcome: request(asks, build, session_id, headers, method, &params).await })
}

fn refused(id: Value, code: Code, message: impl Into<String>) -> Reply {
    Reply::Answer(Answer { id, outcome: Err(RpcError::new(code, message)) })
}

async fn request(
    asks: &Asks,
    build: Build,
    session_id: &SessionId,
    headers: &Headers,
    name: &str,
    params: &Map<String, Value>,
) -> Result<Value, RpcError> {
    let method = Method::named(name);
    if method == Method::Initialize || matches!(&headers.version, Said::Value(v) if v == super::legacy::VERSION) {
        return super::legacy::request(asks, build, session_id, headers, name, params).await;
    }
    agrees("MCP-Protocol-Version", &headers.version, version(params))?;
    agrees("Mcp-Method", &headers.method, Some(name))?;
    let names_a_task = matches!(method, Method::GetTask | Method::UpdateTask | Method::CancelTask);
    if method == Method::CallTool {
        agrees("Mcp-Name", &headers.name, params.get("name").and_then(Value::as_str))?;
    } else if names_a_task && headers.name != Said::Nothing {
        // Only when it's sent: the Claude CLI cancels the task of a call interrupted as it was answered without one,
        // which refusing would leave the wait running.
        agrees("Mcp-Name", &headers.name, task(params))?;
    }
    match version(params) {
        Some(VERSION) => {}
        requested => return Err(unsupported(requested)),
    }
    match method {
        Method::Discover => Ok(discover(build)),
        Method::Ping => Ok(json!({ "resultType": "complete" })),
        Method::ListTools => Ok(json!({
            "resultType": "complete",
            "tools": [pull_requests::definition(false), pull_requests::definition(true)],
            "ttlMs": 0,
            "cacheScope": "public",
        })),
        Method::CallTool => call(asks, session_id, params).await,
        Method::GetTask => wait::get(asks, session_id, task_in(params, name)?).await,
        // A wait never asks for input, so there's nothing to update, but the task has to be the chat's.
        Method::UpdateTask => {
            wait::get(asks, session_id, task_in(params, name)?).await.map(|_| json!({ "resultType": "complete" }))
        }
        Method::CancelTask => wait::cancel(asks, session_id, task_in(params, name)?).await,
        Method::Initialize | Method::Other(_) => {
            Err(RpcError::new(Code::MethodNotFound, format!("there's no method {name} here")))
        }
    }
}

fn version(params: &Map<String, Value>) -> Option<&str> {
    params.get("_meta")?.get(VERSION_KEY)?.as_str()
}

/// The task a request for one names.
fn task(params: &Map<String, Value>) -> Option<&str> {
    params.get("taskId")?.as_str()
}

/// The task a request about one names, from a client that offered to take tasks, as the tasks extension requires.
fn task_in<'a>(params: &'a Map<String, Value>, method: &str) -> Result<&'a str, RpcError> {
    if !takes_tasks(params) {
        return Err(RpcError::needs_tasks());
    }
    task(params).ok_or_else(|| RpcError::new(Code::InvalidParams, format!("{method} names its task")))
}

/// Whether the client offered to take a task for this request, which it says in each one.
fn takes_tasks(params: &Map<String, Value>) -> bool {
    let offered = params.get("_meta").and_then(|meta| meta.get(CAPABILITIES_KEY)?.get("extensions")?.get(TASKS));
    offered.is_some_and(Value::is_object)
}

/// Whether a header says what the body does, as Streamable HTTP has every request say it twice.
pub(super) fn agrees(header: &str, said: &Said, body: Option<&str>) -> Result<(), RpcError> {
    let mismatch = |message: String| Err(RpcError::new(Code::HeaderMismatch, message));
    match (said, body) {
        (Said::Value(said), Some(body)) if said == body => Ok(()),
        (Said::Nothing, _) => mismatch(format!("the {header} header is missing")),
        (Said::Unreadable, _) => mismatch(format!("the {header} header can't be read")),
        (Said::Value(said), Some(body)) => mismatch(format!("the {header} header says {said}, and the body {body}")),
        (Said::Value(said), None) => mismatch(format!("the {header} header says {said}, and the body says nothing")),
    }
}

pub(super) fn unsupported(requested: Option<&str>) -> RpcError {
    RpcError {
        code: Code::UnsupportedVersion,
        message: format!("Unsupported protocol version: this server speaks {VERSION} and {}", super::legacy::VERSION),
        data: Some(json!({ "supported": [VERSION, super::legacy::VERSION], "requested": requested })),
    }
}

/// Never cached for long, since the tools change with the worker's build, which can't say so as it's replaced.
fn discover(build: Build) -> Value {
    json!({
        "resultType": "complete",
        "supportedVersions": [VERSION],
        "capabilities": { "tools": {}, "extensions": { TASKS: {} } },
        "_meta": { "io.modelcontextprotocol/serverInfo": { "name": "roundhouse", "version": build.version() } },
        "instructions": INSTRUCTIONS,
        "ttlMs": 0,
        "cacheScope": "public",
    })
}

async fn call(asks: &Asks, session_id: &SessionId, params: &Map<String, Value>) -> Result<Value, RpcError> {
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return Err(RpcError::new(Code::InvalidParams, "a call names its tool"));
    };
    match Tool::named(name) {
        Some(Tool::PullRequests) => Ok(pull_requests::read(asks, session_id, params).await),
        Some(Tool::Waits | Tool::Wait) if !takes_tasks(params) => Err(RpcError::needs_tasks()),
        Some(Tool::Waits) => Ok(pull_requests::start(asks, session_id, params).await),
        Some(Tool::PullRequest) => Ok(pull_request::call(asks, session_id).await.to_json()),
        // A wait is answered with a task, which a client that didn't offer to take one can't be given.
        Some(Tool::Wait) => Ok(wait::call(asks, session_id, params).await),
        None => Err(RpcError::new(Code::InvalidParams, format!("there's no tool called {name}"))),
    }
}

/// The tools there are, each listed by `tools/list` and answered by `tools/call`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tool {
    PullRequest,
    Wait,
    PullRequests,
    Waits,
}

impl Tool {
    pub(super) fn named(name: &str) -> Option<Self> {
        match name {
            pull_requests::READ => Some(Tool::PullRequests),
            pull_requests::WAIT => Some(Tool::Waits),
            pull_request::NAME => Some(Tool::PullRequest),
            wait::NAME => Some(Tool::Wait),
            _ => None,
        }
    }
}

/// What a tool says back, as text for the agent to read.
pub(in super::super) struct Called {
    pub(super) text: String,
    /// The tool couldn't do what it's for, which the agent reads as its result rather than as MCP failing.
    pub(super) is_error: bool,
}

impl Called {
    pub(in super::super) fn text(text: String) -> Self {
        Self { text, is_error: false }
    }

    pub(in super::super) fn failed(text: String) -> Self {
        Self { text, is_error: true }
    }

    pub(in super::super) fn to_json(&self) -> Value {
        json!({
            "resultType": "complete",
            "content": [{ "type": "text", "text": self.text }],
            "isError": self.is_error,
        })
    }
}
