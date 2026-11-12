//! Where the workspace's pull request stands on GitHub, which the app passes on to each chat, for its line for a wait
//! on it.

use gpui_kit::Context;

use super::Transcript;

/// Where the workspace's pull request stands on GitHub, as the app words it for a wait's line, as "2 checks running".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequestStanding {
    pub number: u64,
    pub words: String,
}

impl Transcript {
    /// Follows where the workspace's pull request stands on GitHub, as the app words it, which a line for a wait on it
    /// says.
    pub fn set_pull_request(&mut self, standing: Option<PullRequestStanding>, cx: &mut Context<Self>) {
        let waiting = {
            let mut m = self.model.borrow_mut();
            if m.pull_request == standing {
                return;
            }
            m.pull_request = standing;
            m.at_work.wait.is_some()
        };
        // Only a wait's line says it, on a line of its own, so no row changes height.
        if waiting {
            cx.notify();
        }
    }
}
