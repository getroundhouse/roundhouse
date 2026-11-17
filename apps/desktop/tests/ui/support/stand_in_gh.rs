//! A real hub with a stand-in for `gh` at the front of its `PATH`, which answers as a test has it.

#![cfg(test)]

use process::testing::WholeEnvironment;

use super::Stack;
use super::hub_ahead::HubAhead;

/// The hub, which goes with the test, even a failed one.
pub(crate) struct StandInGh {
    _hub: HubAhead,
}

impl Stack {
    /// Starts the hub before the app opens, in `environment`, with a `gh` that runs `also`, a line of shell, and fails
    /// at everything else.
    pub(super) fn start_hub_with_gh(&self, environment: &mut WholeEnvironment, also: &str) -> StandInGh {
        let bin = self.dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let gh = bin.join("gh");
        std::fs::write(&gh, format!("#!/bin/sh\n{also}\nexit 1\n")).unwrap();
        std::fs::set_permissions(&gh, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        // Workers get this PATH, with the stand-in at its front.
        let hub = self.start_hub_ahead(environment.put_first_on_path(&bin));
        StandInGh { _hub: hub }
    }
}

impl super::Ui<'_> {
    /// Lets the time pass after which the app reads the pull request showing again, and lets it read it.
    pub(crate) fn wait_out_a_pull_request_look(&mut self) {
        self.cx.executor().advance_clock(desktop::pull_request::READ_AGAIN);
        self.frame();
    }
}
