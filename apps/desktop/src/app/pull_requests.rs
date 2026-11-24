//! What the window shows of the hub beyond its records: every workspace's pull request on its mark, and the selected
//! workspace in full, which is followed at its own level, so its changed files and its pull request's bar come. The app
//! tells client-core, which tells the hub, which reads them as often as that needs and sends what changes, so the app
//! asks nothing again on a timer.

use client_core::{FollowedWorkspace, GithubView, RecordsFeed, ShownOfGithub};
use gpui_kit::{Context, Task};
use protocol::model::{PullRequestFound, Workspace};

use super::AppView;
use crate::workspace_pane::WorkspacePane;

/// What the window shows of the hub beyond its records, and what draws it again as what GitHub says of it changes.
pub(super) struct Shown {
    feed: RecordsFeed,
    view: GithubView,
    /// The selected workspace, followed at its own level.
    in_full: Option<FollowedWorkspace>,
    _listening: Task<()>,
}

impl AppView {
    /// Starts saying, through `feed`, what the window shows, and drawing its pull requests again as the hub sends what
    /// changes.
    pub(super) fn show_pull_requests(&mut self, feed: &RecordsFeed, cx: &mut Context<Self>) {
        let view = feed.github().view();
        let changed = view.changed().clone();
        let listening = cx.spawn(async move |this, cx| {
            while changed.recv_async().await.is_ok() {
                if this.update(cx, AppView::pull_requests_changed).is_err() {
                    return;
                }
            }
        });
        self.shown = Some(Shown { feed: feed.clone(), view, in_full: None, _listening: listening });
        self.say_what_shows();
    }

    /// Says what the window shows now: the pull request of every workspace the app has a row for, in the sidebar or in
    /// Home, which has the hub read nothing more, and the selected workspace in full, which is followed at its own
    /// level, so the hub sends its changed files and its pull request's checks, history and stack, reads its pull
    /// request more often, and has its worker watch its files.
    pub(super) fn say_what_shows(&mut self) {
        let Some(shown) = &mut self.shown else { return };
        let workspaces = self.records.workspaces().iter().map(|workspace| workspace.id.clone()).collect();
        shown.view.show(ShownOfGithub { workspaces, references: Vec::new() });
        let selected = self.selection.workspace();
        if shown.in_full.as_ref().map(FollowedWorkspace::workspace) != selected {
            shown.in_full = selected.map(|id| shown.feed.follow_workspace(id));
        }
    }

    /// `workspace`'s pull request, as the hub sent it. `None` until the hub has looked.
    pub(crate) fn pull_request_of(&self, workspace: &Workspace) -> Option<PullRequestFound> {
        self.shown.as_ref()?.view.feed().pull_request(workspace)
    }

    fn pull_requests_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.selected_pane() {
            pane.update(cx, WorkspacePane::take_pull_request);
        }
        cx.notify();
    }
}
