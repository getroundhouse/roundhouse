//! The MCP server `--mcp-config` names, reached as the real CLI reaches one over HTTP, on protocol `2026-07-28`, with
//! every header the version asks for. As it launches, it reads the file once, and asks the server `server/discover`
//! and `tools/list`, and a server that refuses either has failed, as the real CLI marks it. `mcp:TOOL` calls one of
//! its tools, and what the tool says comes back as the call's result. With `CLAUDE_AUTO_BACKGROUND_TASKS=1`, a call
//! offers to take an MCP task, and one answered with a task goes on in the background, as the real CLI's does.
//!
//! `crates/worker/tests/serve.rs` writes the same request by hand, so a change to what the real CLI sends goes in both.

use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Value, json};

use crate::settings::flag;
use crate::turn::Turn;
use crate::{Config, Out, background};

/// The server the hub's worker serves, as the agent adapter names it.
const SERVER: &str = "roundhouse";

const VERSION: &str = "2026-07-28";

/// MCP's tasks extension, which the real CLI offers in every request's `_meta` but a call's, and in a call's too with
/// [`TAKES_TASKS`] set.
const TASKS: &str = "io.modelcontextprotocol/tasks";

const TAKES_TASKS: &str = "CLAUDE_AUTO_BACKGROUND_TASKS";

/// The real CLI's bounds on how often it asks after a task, and what it takes when the server doesn't say.
const POLL_BOUNDS_MS: (u64, u64) = (250, 30_000);
const POLL_DEFAULT_MS: u64 = 2_000;

/// Longer than the hub takes over an agent's read, which can wait 10 s after another, so a server that's stuck fails
/// the call that reached it, rather than the test waiting on the turn.
const PATIENCE: Duration = Duration::from_secs(15);

/// The server, once it has answered, or why it failed.
static CONNECTED: OnceLock<Result<Server, String>> = OnceLock::new();

/// Held while the tasks kept beside the conversation are read and written again, since each is followed on a thread
/// of its own.
static KEEPING: Mutex<()> = Mutex::new(());

struct Server {
    address: String,
    path: String,
    authorization: String,
    tools: Vec<String>,
}

/// Connects to the server `--mcp-config` names, if it names one, and keeps a copy of the file as `last-mcp-config`, so
/// a test can see what each launch was given, since the adapter removes the file once the agent exits. A launch given
/// none leaves none.
pub(crate) fn connect(args: &[String], state: &Path) {
    let kept = state.join("last-mcp-config");
    let Some(config) = flag(args, "--mcp-config") else {
        let _ = std::fs::remove_file(kept);
        return;
    };
    std::fs::copy(&config, kept).unwrap();
    let config: Value = serde_json::from_slice(&std::fs::read(config).unwrap()).unwrap();
    let server = &config["mcpServers"][SERVER];
    let url = server["url"].as_str().unwrap();
    let (address, path) = url.strip_prefix("http://").unwrap().split_once('/').unwrap();
    let mut server = Server {
        address: address.to_string(),
        path: format!("/{path}"),
        authorization: server["headers"]["Authorization"].as_str().unwrap().to_string(),
        tools: Vec::new(),
    };
    let connected = server.request("server/discover", None, json!({})).and_then(|_| {
        let listed = server.request("tools/list", None, json!({}))?;
        let tools = listed["tools"].as_array().ok_or(format!("{SERVER} listed no tools"))?;
        server.tools = tools.iter().filter_map(|tool| tool["name"].as_str().map(String::from)).collect();
        Ok(server)
    });
    let _ = CONNECTED.set(connected);
}

/// Calls `tool`, which the model knows as `mcp__roundhouse__TOOL`, as the real CLI names a server's tools.
pub(crate) fn call(turn: &mut Turn, tool: &str) {
    let tool_use_id = turn.named_tool_use(&format!("mcp__{SERVER}__{tool}"), json!({}));
    match ask(tool, &tool_use_id) {
        Ok(Answered::Result(text, is_error)) => turn.tool_result(&tool_use_id, &text, is_error),
        Ok(Answered::Task(task)) => in_the_background(turn, tool, &tool_use_id, &task),
        Err(failed) => turn.tool_result(&tool_use_id, &failed, true),
    }
}

/// How a server answered a call.
enum Answered {
    /// What the tool said, and whether it failed.
    Result(String, bool),
    /// An MCP task, which ends later.
    Task(Value),
}

/// The server, once it has answered. `Err` with why when it can't be reached, as the real CLI says when a server
/// isn't there.
fn server() -> Result<&'static Server, String> {
    CONNECTED.get().ok_or("no MCP server was given")?.as_ref().map_err(|failed| format!("{SERVER} failed: {failed}"))
}

/// How the server answered a call to `tool`, for the agent's call `tool_use_id`. `Err` with why when it can't be
/// called, as the real CLI says when the server doesn't have the tool.
fn ask(tool: &str, tool_use_id: &str) -> Result<Answered, String> {
    let server = server()?;
    if !server.tools.iter().any(|listed| listed == tool) {
        return Err(format!("{SERVER} has no tool {tool}"));
    }
    let params = json!({ "name": tool, "arguments": {}, "_meta": { "claudecode/toolUseId": tool_use_id } });
    let result = server.request("tools/call", Some(tool), params)?;
    if result["resultType"] == "task" {
        return Ok(Answered::Task(result));
    }
    let text = result["content"][0]["text"].as_str().ok_or(format!("{SERVER} said nothing"))?;
    Ok(Answered::Result(text.to_string(), result["isError"].as_bool().unwrap_or(false)))
}

/// A task the agent follows, kept beside its conversation until it ends, as the real CLI keeps one, so a launch with
/// `--resume` follows it again.
#[derive(Clone)]
struct Followed {
    /// The CLI's own ID for it.
    id: String,
    /// The server's.
    task_id: String,
    tool_use_id: String,
    description: String,
    /// Where the server was, and the token it was reached with. A launch follows the task again only at the same, as
    /// is all that's been seen of the real CLI, which the hub gives it.
    url: String,
    authorization: String,
}

/// Lists the task in the background, as the real CLI does, with the call's result saying so, and follows it.
fn in_the_background(turn: &mut Turn, tool: &str, tool_use_id: &str, task: &Value) {
    let server = server().unwrap();
    let followed = Followed {
        id: format!("task_{tool_use_id}"),
        task_id: task["taskId"].as_str().unwrap().to_string(),
        tool_use_id: tool_use_id.to_string(),
        description: format!("{SERVER}/{tool}"),
        url: server.url(),
        authorization: server.authorization.clone(),
    };
    background::started(turn.config, turn.out, &followed.id, "mcp_task", tool_use_id, &followed.description);
    background::watch_for_sigterm(turn.config, turn.out);
    turn.tool_result(tool_use_id, &format!("The task {} was moved to the background.", followed.task_id), false);
    change_kept(turn.config, |kept| kept.push(followed.clone()));
    follow(turn.config, turn.out, followed, poll_interval(task));
}

/// On `--resume`, lists again the tasks a launch before this one left, before any prompt, with the `task_started` of
/// each, and follows them, as the real CLI does.
pub(crate) fn follow_again(args: &[String], config: &Config, out: &Out) {
    let Ok(server) = server() else { return };
    if flag(args, "--resume").is_none() {
        return;
    }
    let kept = {
        let _keeping = KEEPING.lock();
        kept(config)
    };
    let at_the_same =
        |followed: &Followed| followed.url == server.url() && followed.authorization == server.authorization;
    for followed in kept.into_iter().filter(at_the_same) {
        background::started(config, out, &followed.id, "mcp_task", &followed.tool_use_id, &followed.description);
        background::watch_for_sigterm(config, out);
        follow(config, out, followed, Duration::from_millis(POLL_DEFAULT_MS));
    }
}

/// Asks after the task every `pollIntervalMs` the server last gave, asking again after an error. Once it has
/// completed, the agent picks up again by itself and says the first line of what the tool said, as the CLI starts a
/// turn with a `<task-notification>`. Once the server has called it off, the CLI says it failed, and the agent picks up
/// again the same way, saying what the server last said of it.
fn follow(config: &Config, out: &Out, followed: Followed, mut poll: Duration) {
    let (config, out) = (config.clone(), out.clone());
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(poll);
            let asked = json!({ "taskId": followed.task_id });
            let Ok(task) = server().and_then(|server| server.request("tasks/get", Some(&followed.task_id), asked))
            else {
                continue;
            };
            poll = poll_interval(&task);
            let (status, said) = match task["status"].as_str() {
                Some("completed") => ("completed", task["result"]["content"][0]["text"].as_str()),
                // As the real CLI has it, which tells the agent what the server last said of the task.
                Some("cancelled" | "failed") => ("failed", task["statusMessage"].as_str()),
                _ => continue,
            };
            let told = format!("say:{}", said.unwrap_or_default().lines().next().unwrap_or_default().replace(';', ","));
            change_kept(&config, |kept| kept.retain(|other| other.id != followed.id));
            return background::finished(&config, &out, &followed.id, status, told, Duration::ZERO);
        }
    });
}

fn kept_at(config: &Config) -> PathBuf {
    config.conversation.with_extension("mcp-tasks")
}

/// Changes the tasks kept, read and written again whole.
fn change_kept(config: &Config, change: impl FnOnce(&mut Vec<Followed>)) {
    let _keeping = KEEPING.lock();
    let mut kept = kept(config);
    change(&mut kept);
    keep(config, &kept);
}

fn kept(config: &Config) -> Vec<Followed> {
    let kept: Vec<Value> =
        std::fs::read(kept_at(config)).ok().and_then(|kept| serde_json::from_slice(&kept).ok()).unwrap_or_default();
    let field = |followed: &Value, name: &str| followed[name].as_str().unwrap_or_default().to_string();
    kept.iter()
        .map(|followed| Followed {
            id: field(followed, "id"),
            task_id: field(followed, "task_id"),
            tool_use_id: field(followed, "tool_use_id"),
            description: field(followed, "description"),
            url: field(followed, "url"),
            authorization: field(followed, "authorization"),
        })
        .collect()
}

fn keep(config: &Config, kept: &[Followed]) {
    let kept: Vec<_> = kept
        .iter()
        .map(|followed| {
            json!({ "id": followed.id, "task_id": followed.task_id, "tool_use_id": followed.tool_use_id,
                "description": followed.description, "url": followed.url, "authorization": followed.authorization })
        })
        .collect();
    std::fs::write(kept_at(config), serde_json::to_vec(&kept).unwrap()).unwrap();
}

fn poll_interval(task: &Value) -> Duration {
    let (least, most) = POLL_BOUNDS_MS;
    Duration::from_millis(task["pollIntervalMs"].as_u64().unwrap_or(POLL_DEFAULT_MS).clamp(least, most))
}

impl Server {
    fn url(&self) -> String {
        format!("http://{}{}", self.address, self.path)
    }

    /// Sends one request and returns its result, or why there wasn't one. `name` goes in `Mcp-Name`, which the real
    /// CLI sends with a call and with a request about a task.
    fn request(&self, method: &str, name: Option<&str>, mut params: Value) -> Result<Value, String> {
        let offers_tasks = method != "tools/call" || std::env::var(TAKES_TASKS).is_ok_and(|set| set == "1");
        let meta = params.as_object_mut().unwrap().entry("_meta").or_insert_with(|| json!({}));
        meta["io.modelcontextprotocol/protocolVersion"] = json!(VERSION);
        if offers_tasks {
            meta["io.modelcontextprotocol/clientCapabilities"] = json!({ "extensions": { TASKS: {} } });
        }
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string();
        let name = name.map(|name| format!("Mcp-Name: {name}\r\n")).unwrap_or_default();
        let request = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nAuthorization: {}\r\nContent-Type: application/json\r\n\
             Accept: application/json, text/event-stream\r\nMCP-Protocol-Version: {VERSION}\r\nMcp-Method: {method}\r\n\
             {name}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.path,
            self.address,
            self.authorization,
            body.len(),
        );
        let address: SocketAddr = self.address.parse().map_err(|_| format!("{SERVER} is at {}", self.address))?;
        let mut stream =
            TcpStream::connect_timeout(&address, PATIENCE).map_err(|e| format!("{SERVER} can't be reached: {e}"))?;
        stream
            .set_read_timeout(Some(PATIENCE))
            .and(stream.set_write_timeout(Some(PATIENCE)))
            .map_err(|e| e.to_string())?;
        stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
        let mut response = String::new();
        stream.read_to_string(&mut response).map_err(|e| e.to_string())?;
        let (head, body) = response.split_once("\r\n\r\n").ok_or(format!("{SERVER} answered {response:?}"))?;
        let reply: Value = serde_json::from_str(body).map_err(|_| format!("{SERVER} answered {head}"))?;
        if reply["result"].is_null() {
            return Err(format!("{SERVER} refused {method}: {}", reply["error"]));
        }
        Ok(reply["result"].clone())
    }
}
