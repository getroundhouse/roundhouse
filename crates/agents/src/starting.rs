//! Starting an agent's CLI, for a session or a one-off call, which every kind of agent does here, so each start has
//! what its worker gives every process it starts.

use std::path::Path;

use process::Environment;
use tokio::process::{Child, Command};

use crate::AgentError;

/// Starts `command`, which runs `bin` in `cwd`, with `environment` on it last, so nothing the launch set can undo it,
/// and says which if it can't.
pub(crate) fn start(
    command: &mut Command,
    bin: &Path,
    cwd: &Path,
    environment: &Environment,
) -> Result<Child, AgentError> {
    environment.apply(command.as_std_mut());
    command.spawn().map_err(|source| AgentError::Start {
        bin: bin.display().to_string(),
        cwd: cwd.display().to_string(),
        source,
    })
}
