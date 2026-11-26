use crate::model::{
    Rev, ToolServer, ToolServerAuth, ToolServerHeader, ToolServerName, ToolServerUrl, ToolSignInStatus,
};

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

/// The tool server `id`, as [`tool_server`] makes it, whose sign-in is gone, so the person has to sign in to it.
pub fn tool_server_needing_sign_in(id: &str) -> ToolServer {
    signed_in_to(id, ToolSignInStatus::NeedsSignIn)
}

/// The tool server `id`, as [`tool_server`] makes it, whose sign-in works, and which wants more access than it has.
pub fn tool_server_wanting_more_access(id: &str) -> ToolServer {
    signed_in_to(id, ToolSignInStatus::NeedsMoreAccess)
}

fn signed_in_to(id: &str, status: ToolSignInStatus) -> ToolServer {
    ToolServer { auth: ToolServerAuth::SignIn { status }, ..tool_server(id) }
}
