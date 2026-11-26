use std::path::{Path, PathBuf};
use std::sync::Arc;

use protocol::client::tool_servers::{ToolServerAdd, ToolServerEdit};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ToolServerAuthGiven, ToolServerId, ToolServerKey};

use super::super::ToolServers;
use crate::store::Store;
use crate::store::tests::open_in;

/// The tool servers of a hub whose data directory is `dir`, and its store.
pub(super) fn servers_in(dir: &Path) -> (ToolServers, Arc<Store>) {
    let store = Arc::new(open_in(dir));
    (ToolServers::new(store.clone(), dir), store)
}

/// Adding `name` as `id`, with the key `key` in `Authorization`.
pub(crate) fn with_key(id: &str, name: &str, key: &str) -> ToolServerAdd {
    let auth = ToolServerAuthGiven::Key { header: "Authorization".into(), key: ToolServerKey::new(key) };
    ToolServerAdd { auth, ..with_neither(id, name) }
}

pub(super) fn with_neither(id: &str, name: &str) -> ToolServerAdd {
    let url = "https://mcp.example.com/mcp".into();
    ToolServerAdd { id: id.into(), name: name.into(), url, auth: ToolServerAuthGiven::Neither }
}

/// An edit of the server `id` that changes nothing.
pub(crate) fn edit(id: &str) -> ToolServerEdit {
    ToolServerEdit { id: id.into(), name: None, url: None, auth: None, saved_anyway: false }
}

/// Having the server `id` called `name`.
pub(crate) fn named(id: &str, name: &str) -> ToolServerEdit {
    ToolServerEdit { name: Some(name.into()), ..edit(id) }
}

/// Having no change to a server's row be written, as a hub that stops before one is.
pub(super) fn stop_writing_rows(store: &Store) {
    let stop = "CREATE TEMP TRIGGER stopped BEFORE UPDATE ON tool_servers BEGIN SELECT RAISE(ABORT, 'stopped'); END";
    store.holding_writes().execute_batch(stop).unwrap();
}

/// The servers workers are sent, by ID, and what each request to each carries.
pub(super) fn sent(servers: &ToolServers) -> Vec<(String, Option<String>)> {
    let set = servers.sent().borrow().set();
    let carried = |server: protocol::worker::ProxiedServer| {
        let header = server.header.map(|header| format!("{}: {}", header.name.as_str(), header.value.as_str()));
        (server.id.to_string(), header)
    };
    set.servers.into_iter().map(carried).collect()
}

pub(super) fn refused<T: std::fmt::Debug>(done: Result<T, ApiError>) -> ErrorCode {
    done.unwrap_err().code
}

/// What's in every file in the tool servers' directory.
pub(super) fn files_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir.join("tool-servers")) else { return Vec::new() };
    let mut files: Vec<String> = entries.map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap()).collect();
    files.sort();
    files
}

/// The file of the server `id`.
pub(super) fn path(dir: &Path, store: &Store, id: &ToolServerId) -> PathBuf {
    let file = store.tool_server(id).unwrap().unwrap().1.unwrap();
    dir.join("tool-servers").join(file.as_str())
}

/// Every file in `dir` that holds `text`, such as the database, its log and whatever else the hub keeps, by the
/// directory it's in.
pub(super) fn holding(dir: &Path, text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(holding(&path, text));
        } else if std::fs::read(&path).unwrap().windows(text.len()).any(|bytes| bytes == text.as_bytes()) {
            found.push(path.parent().unwrap().file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    found
}

pub(super) const KEY_FILE: &str = r#"{"format":"key","key":"Bearer made-up"}"#;
