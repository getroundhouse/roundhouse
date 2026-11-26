//! What the window shows of the hub beyond what its level sends: the selected workspace in full, which is followed at
//! its own level, so its changed files and its pull request's bar come. Every workspace's pull request, which its mark
//! shows, comes with the hub's level, which client-core keeps followed. The app tells client-core, which tells the hub,
//! which reads them as often as that needs and sends what changes, so the app asks nothing again on a timer.

use client_core::{FollowedWorkspace, RecordsFeed};

use super::AppView;

/// What the window shows of the hub beyond what its level sends.
pub(super) struct Shown {
    feed: RecordsFeed,
    /// The selected workspace, followed at its own level.
    in_full: Option<FollowedWorkspace>,
}

impl AppView {
    /// Starts saying, through `feed`, what the window shows.
    pub(super) fn start_showing(&mut self, feed: &RecordsFeed) {
        self.shown = Some(Shown { feed: feed.clone(), in_full: None });
        self.say_what_shows();
    }

    /// Says what the window shows now: the selected workspace in full, which is followed at its own level, so the hub
    /// sends its changed files and its pull request's checks, history and stack, reads its pull request more often, and
    /// has its worker watch its files.
    pub(super) fn say_what_shows(&mut self) {
        let Some(shown) = &mut self.shown else { return };
        let selected = self.selection.workspace();
        if shown.in_full.as_ref().map(FollowedWorkspace::workspace) != selected {
            shown.in_full = selected.map(|id| shown.feed.follow_workspace(id));
        }
    }
}
