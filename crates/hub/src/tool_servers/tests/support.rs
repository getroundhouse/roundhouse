use std::path::Path;
use std::sync::Arc;

use protocol::client::tool_servers::ToolServerAdd;
use protocol::model::{ToolServerAuthGiven, ToolServerKey};

use super::super::ToolServers;
use crate::store::Store;
use crate::store::tests::open_in;

/// The tool servers of a hub whose data directory is `dir`, and its store.
pub(super) fn servers_in(dir: &Path) -> (ToolServers, Arc<Store>) {
    let store = Arc::new(open_in(dir));
    (ToolServers::new(store.clone(), dir), store)
}

/// Adding `name` as `id`, with the key `key` in `Authorization`.
pub(super) fn with_key(id: &str, name: &str, key: &str) -> ToolServerAdd {
    let auth = ToolServerAuthGiven::Key { header: "Authorization".into(), key: ToolServerKey::new(key) };
    ToolServerAdd { auth, ..with_neither(id, name) }
}

pub(super) fn with_neither(id: &str, name: &str) -> ToolServerAdd {
    let url = "https://mcp.example.com/mcp".into();
    ToolServerAdd { id: id.into(), name: name.into(), url, auth: ToolServerAuthGiven::Neither }
}
