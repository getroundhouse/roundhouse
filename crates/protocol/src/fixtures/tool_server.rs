use crate::model::{Rev, ToolServer, ToolServerAuth, ToolServerHeader, ToolServerName, ToolServerUrl};

/// The tool server `id`, called `Docs` and its ID, at an address of its own, with a key in `Authorization`.
#[expect(clippy::unwrap_used, reason = "Authorization is a header a key can be sent in")]
pub fn tool_server(id: &str) -> ToolServer {
    let header = ToolServerHeader::typed("Authorization").unwrap();
    ToolServer {
        id: id.into(),
        name: ToolServerName::stored(format!("Docs {id}")),
        url: ToolServerUrl::stored(format!("https://{id}.example.com/mcp")),
        auth: ToolServerAuth::Key { header },
        rev: Rev::default(),
    }
}
