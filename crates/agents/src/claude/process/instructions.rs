//! Telling the CLI Roundhouse's instructions, after its own system prompt and each of its subagents', in the file
//! [`Instructions`] names, which keeps them off its command line.

use std::ffi::OsString;
use std::io;
use std::path::Path;

use crate::{AgentError, Instructions};

/// Adds what a file says to the agent's system prompt, and to each of its subagents'. A subagent gets none of the
/// agent's own, so it's told with a flag of its own. `--help` names neither, and a CLI without either, or given a file
/// that isn't there, refuses to start.
const FLAGS: [&str; 2] = ["--append-system-prompt-file", "--append-subagent-system-prompt-file"];

/// The CLI's arguments that tell it `told`, once the file they name is written. Every launch is given them, resumed ones
/// included: the CLI keeps the text a conversation started with, but once one is compacted it takes what the launch
/// that resumed it gave, as `claude --help` says. A file that can't be written fails the launch rather than leave the
/// agent untold, since a chat goes on with what it was first told.
pub(super) fn args(told: &Instructions) -> Result<Vec<OsString>, AgentError> {
    write(&told.file, &told.text).map_err(|source| AgentError::Instructions { path: told.file.clone(), source })?;
    Ok(FLAGS.into_iter().flat_map(|flag| [flag.into(), told.file.clone().into_os_string()]).collect())
}

/// Written again at each launch, the same each time, so a launch started again finds it.
fn write(file: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, text)
}
