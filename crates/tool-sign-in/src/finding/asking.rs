use serde_json::{Value, json};

use crate::challenge::Challenge;
use crate::fetch::Caller;
use crate::{Failure, ServerUrl};

/// The version of MCP whose authorization this follows.
const VERSION: &str = "2026-07-28";

/// The last version with a handshake, for a server that takes none since.
const VERSION_WITH_HANDSHAKE: &str = "2025-11-25";

/// Asks `server` something as nobody, as a client of this version would first, and, where it refuses that for what was
/// asked rather than who asked, as one of the last version with a handshake would, the order MCP gives. A server may
/// check either before it asks for a sign-in, so a refusal of one doesn't say that it asks for none. Gives what it says
/// as it asks for a sign-in, or nothing where it answers anyone.
pub(super) async fn as_nobody(calls: &Caller<'_>, server: &ServerUrl) -> Result<Option<Challenge>, Failure> {
    let headers = [("mcp-protocol-version", VERSION), ("mcp-method", "server/discover")];
    let answer = calls.post_message(server.url(), &headers, &discover()).await?;
    let answer = match answer.status {
        400 | 404 | 405 | 406 | 415 | 422 => calls.post_message(server.url(), &[], &initialize()).await?,
        _ => answer,
    };
    if answer.succeeded() {
        return Ok(None);
    }
    match answer.status {
        401 => Ok(Some(Challenge::of(&answer.headers))),
        _ => Err(answer.failure()),
    }
}

fn discover() -> Value {
    let about = json!({
        "io.modelcontextprotocol/protocolVersion": VERSION,
        "io.modelcontextprotocol/clientInfo": client(),
        "io.modelcontextprotocol/clientCapabilities": {},
    });
    json!({"jsonrpc": "2.0", "id": 1, "method": "server/discover", "params": {"_meta": about}})
}

fn initialize() -> Value {
    let params = json!({"protocolVersion": VERSION_WITH_HANDSHAKE, "clientInfo": client(), "capabilities": {}});
    json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": params})
}

fn client() -> Value {
    json!({"name": "roundhouse", "version": env!("CARGO_PKG_VERSION")})
}
