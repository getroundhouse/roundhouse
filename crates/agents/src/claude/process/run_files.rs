//! The files a run of the CLI keeps on disk while it runs.

use std::path::PathBuf;

use crate::AgentOptions;

/// The record of a run's process, so the provider can stop it if the worker is killed, the file that tells its CLI
/// where its worker's tool servers are, which holds the agent's token, and the one its CLI reads Roundhouse's
/// instructions from. Removed when this is dropped: as the run ends, when it never started, and when supervising it
/// panicked. A launch started again writes them again, where they were.
pub(super) struct RunFiles {
    pid_file: Option<PathBuf>,
    tools_config: Option<PathBuf>,
    instructions: Option<PathBuf>,
}

impl RunFiles {
    pub(super) fn of(options: &AgentOptions) -> Self {
        Self {
            pid_file: options.pid_file.clone(),
            tools_config: options.tools.as_ref().map(|tools| tools.config.clone()),
            instructions: options.instructions.as_ref().map(|told| told.file.clone()),
        }
    }
}

impl Drop for RunFiles {
    fn drop(&mut self) {
        for path in [&self.pid_file, &self.instructions].into_iter().flatten() {
            let _ = std::fs::remove_file(path);
        }
        if let Some(path) = &self.tools_config {
            super::tools::forget(path);
        }
    }
}
