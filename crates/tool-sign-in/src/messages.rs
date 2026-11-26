//! What an MCP server is asked, as a client of each version of MCP asks it: one on 2026-07-28, which says who it is
//! with each message, and one on the last version with a handshake, for a server that takes none since.

use serde_json::{Value, json};

/// The version of MCP whose authorization this follows.
const VERSION: &str = "2026-07-28";

/// The last version with a handshake.
pub(crate) const VERSION_WITH_HANDSHAKE: &str = "2025-11-25";

/// The header a client says its version in, and each of a session's messages the session's.
pub(crate) const VERSION_HEADER: &str = "mcp-protocol-version";

/// A message, with the headers that say what's in it.
pub(crate) struct Message {
    pub(crate) headers: &'static [(&'static str, &'static str)],
    pub(crate) body: Value,
}

impl Message {
    /// What every message is sent with.
    pub(crate) const EVERY_HAS: [(&'static str, &'static str); 2] =
        [("accept", "application/json, text/event-stream"), ("content-type", "application/json")];

    /// What a server is, which a client of this version asks first.
    pub(crate) fn discover() -> Self {
        Self { headers: &[(VERSION_HEADER, VERSION), ("mcp-method", "server/discover")], body: of("server/discover") }
    }

    /// What tools it has, as a client of this version asks.
    pub(crate) fn list_tools() -> Self {
        Self { headers: &[(VERSION_HEADER, VERSION), ("mcp-method", "tools/list")], body: of("tools/list") }
    }

    /// A handshake's first message.
    pub(crate) fn initialize() -> Self {
        let params = json!({"protocolVersion": VERSION_WITH_HANDSHAKE, "clientInfo": client(), "capabilities": {}});
        Self { headers: &[], body: request("initialize", &params) }
    }

    /// A handshake's last.
    pub(crate) fn initialized() -> Self {
        Self { headers: &[], body: json!({"jsonrpc": "2.0", "method": "notifications/initialized"}) }
    }

    /// What tools it has, as a client that has shaken hands asks.
    pub(crate) fn list_tools_after_handshake() -> Self {
        Self { headers: &[], body: request("tools/list", &json!({})) }
    }
}

/// Whether an answer of `status` refuses a message for what was asked rather than who asked, as a server that doesn't
/// take this version may, so it's asked as a client of the last version with a handshake would, the order MCP gives.
pub(crate) fn refuses_what_was_asked(status: u16) -> bool {
    matches!(status, 400 | 404 | 405 | 406 | 415 | 422)
}

/// A request of this version's, which says who asks.
fn of(method: &str) -> Value {
    let about = json!({
        "io.modelcontextprotocol/protocolVersion": VERSION,
        "io.modelcontextprotocol/clientInfo": client(),
        "io.modelcontextprotocol/clientCapabilities": {},
    });
    request(method, &json!({"_meta": about}))
}

/// Each is the only one on its connection, so they're all called the same.
fn request(method: &str, params: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
}

fn client() -> Value {
    json!({"name": "roundhouse", "version": env!("CARGO_PKG_VERSION")})
}
