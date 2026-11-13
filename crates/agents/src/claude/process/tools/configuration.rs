use crate::ToolServer;
use crate::claude::TOOLS_SERVER;
use serde_json::json;
use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::io::Write as _;
use std::path::Path;
use tracing::warn;

/// With it set, the CLI offers MCP's tasks with each call to a tool, so a tool that waits, such as on a pull request,
/// can answer with a task, and the turn can end while it waits. Without it, a call is held until the tool answers.
pub(in super::super) const TAKES_TASKS: &str = "CLAUDE_AUTO_BACKGROUND_TASKS";

/// The CLI's arguments for the worker's tools, once the file that names them is written. `None` if it can't be, and
/// the agent goes without them, as it does when the worker can't serve them.
pub(in super::super) fn args(tools: &ToolServer) -> Option<[OsString; 2]> {
    match write(tools) {
        Ok(()) => Some(["--mcp-config".into(), tools.config.clone().into_os_string()]),
        Err(e) => {
            warn!(
                "the agent goes without the worker's tools, since {} couldn't be written: {e}",
                tools.config.display()
            );
            None
        }
    }
}

/// Written again at each launch, the same each time, so a launch started again finds it.
fn write(tools: &ToolServer) -> io::Result<()> {
    let config = json!({
        "mcpServers": {
            TOOLS_SERVER: {
                "type": "http",
                "url": tools.url,
                "headers": { "Authorization": format!("Bearer {}", tools.token.as_str()) },
            }
        }
    });
    if let Some(dir) = tools.config.parent() {
        std::fs::create_dir_all(dir)?;
    }
    open_for_its_user(&tools.config)?.write_all(&serde_json::to_vec(&config)?)
}

/// Made anew, only its user's, since one left from before may have been readable by others, or opened by something
/// before it held the token, which would read it once it's written.
fn open_for_its_user(path: &Path) -> io::Result<File> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        Ok(()) | Err(_) => {}
    }
    private_files::file_options().write(true).create_new(true).open(path)
}

/// Removes the file at `config`, once nothing will read it again. One that can't be removed leaves the token on disk,
/// so that's said.
pub(in super::super) fn forget(config: &Path) {
    match std::fs::remove_file(config) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => {
            warn!("couldn't remove {}, which holds an agent's token: {e}", config.display());
        }
        Ok(()) | Err(_) => {}
    }
}
