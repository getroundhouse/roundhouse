//! The MCP server `--mcp-config` names, reached as the real CLI reaches one over HTTP, on protocol `2026-07-28`, with
//! every header the version asks for. As it launches, it reads the file once, and asks the server `server/discover`
//! and `tools/list`, and a server that refuses either has failed, as the real CLI marks it. `mcp:TOOL` calls one of
//! its tools, and what the tool says comes back as the call's result.
//!
//! `crates/worker/tests/serve.rs` writes the same request by hand, so a change to what the real CLI sends goes in both.

use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{Value, json};

use crate::settings::flag;
use crate::turn::Turn;

/// The server the hub's worker serves, as the agent adapter names it.
const SERVER: &str = "roundhouse";

const VERSION: &str = "2026-07-28";

/// Longer than the hub takes over an agent's read, which can wait 10 s after another, so a server that's stuck fails
/// the call that reached it, rather than the test waiting on the turn.
const PATIENCE: Duration = Duration::from_secs(15);

/// The server, once it has answered, or why it failed.
static CONNECTED: OnceLock<Result<Server, String>> = OnceLock::new();

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
    let (text, is_error) = ask(tool).unwrap_or_else(|failed| (failed, true));
    turn.tool_result(&tool_use_id, &text, is_error);
}

/// What the tool said, and whether it failed. `Err` with why when it can't be called, as the real CLI says when a
/// server isn't there or doesn't have the tool.
fn ask(tool: &str) -> Result<(String, bool), String> {
    let server = CONNECTED
        .get()
        .ok_or("no MCP server was given")?
        .as_ref()
        .map_err(|failed| format!("{SERVER} failed: {failed}"))?;
    if !server.tools.iter().any(|listed| listed == tool) {
        return Err(format!("{SERVER} has no tool {tool}"));
    }
    let result = server.request("tools/call", Some(tool), json!({ "name": tool, "arguments": {} }))?;
    let text = result["content"][0]["text"].as_str().ok_or(format!("{SERVER} said nothing"))?;
    Ok((text.to_string(), result["isError"].as_bool().unwrap_or(false)))
}

impl Server {
    /// Sends one request and returns its result, or why there wasn't one. `name` goes in `Mcp-Name`, which the real
    /// CLI sends with a call.
    fn request(&self, method: &str, name: Option<&str>, mut params: Value) -> Result<Value, String> {
        params["_meta"] = json!({ "io.modelcontextprotocol/protocolVersion": VERSION });
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
