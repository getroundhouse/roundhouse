//! Telling the CLI where the worker's tools are, in a file given to `--mcp-config`, which keeps the token off its
//! command line, where any process on the machine can read it.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, Write as _};
use std::path::Path;

use serde_json::json;
use tracing::warn;

use crate::ToolServer;
use crate::claude::TOOLS_SERVER;

#[cfg(test)]
mod tests;

/// With it set, the CLI offers MCP's tasks with each call to a tool, so a tool that waits, such as on a pull request,
/// can answer with a task, and the turn can end while it waits. Without it, a call is held until the tool answers.
pub(super) const TAKES_TASKS: &str = "CLAUDE_AUTO_BACKGROUND_TASKS";

/// The CLI's arguments for the worker's tools, once the file that names them is written. `None` if it can't be, and
/// the agent goes without them, as it does when the worker can't serve them.
pub(super) fn args(tools: &ToolServer) -> Option<[OsString; 2]> {
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

/// Made anew, since one left from before may have been readable by others, or opened by something before it held the
/// token, which would read it once it's written. On Unix only its user can read it. On Windows a new file takes the
/// permissions of its folder, which in the default data directory are the user's alone.
fn open_for_its_user(path: &Path) -> io::Result<File> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        Ok(()) | Err(_) => {}
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

/// Removes the file at `config`, once nothing will read it again. One that can't be removed leaves the token on disk,
/// so that's said.
pub(super) fn forget(config: &Path) {
    match std::fs::remove_file(config) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => {
            warn!("couldn't remove {}, which holds an agent's token: {e}", config.display());
        }
        Ok(()) | Err(_) => {}
    }
}
