//! The files a run of the CLI keeps on disk while it runs.

use std::path::PathBuf;

use crate::AgentOptions;

/// The record of a run's process, so the provider can stop it if the worker is killed, and the file that tells its CLI
/// where the worker's tools are, which holds the agent's token. Removed when this is dropped: as the run ends, when it
/// never started, and when supervising it panicked. A launch started again writes both again, where they were.
pub(super) struct RunFiles {
    pid_file: Option<PathBuf>,
    tools_config: Option<PathBuf>,
}

impl RunFiles {
    pub(super) fn of(options: &AgentOptions) -> Self {
        Self {
            pid_file: options.pid_file.clone(),
            tools_config: options.tools.as_ref().map(|tools| tools.config.clone()),
        }
    }
}

impl Drop for RunFiles {
    fn drop(&mut self) {
        if let Some(path) = &self.pid_file {
            let _ = std::fs::remove_file(path);
        }
        if let Some(path) = &self.tools_config {
            super::tools::forget(path);
        }
    }
}
