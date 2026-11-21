//! What the window shows of workspaces' pull requests: every workspace's on its mark, and the one showing in full in
//! its bar. The app tells the hub, which reads them as often as that needs and sends what changes, so the app
//! asks nothing again on a timer.

use client_core::{GithubView, RecordsFeed};
use gpui_kit::{Context, Task};
use protocol::client::github::GithubShown;
use protocol::model::{PullRequestFound, Workspace};

use super::AppView;
use crate::workspace_pane::WorkspacePane;

/// The window's part of what the hub says of GitHub, and what draws it again as that changes.
pub(super) struct ShownOfGithub {
    view: GithubView,
    _listening: Task<()>,
}

impl AppView {
    /// Starts telling the hub, through `feed`, which pull requests the window shows, and drawing them again as it sends
    /// what changes.
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
        self.shown_of_github = Some(ShownOfGithub { view, _listening: listening });
        self.say_which_pull_requests_show();
    }

    /// Tells the hub which pull requests show now: every workspace's the app has a row for, in the sidebar or in Home,
    /// which costs no more reads, and the selected one's in full.
    pub(super) fn say_which_pull_requests_show(&self) {
        let Some(shown) = &self.shown_of_github else { return };
        let workspaces = self.records.workspaces().iter().map(|workspace| workspace.id.clone()).collect();
        let closely = self.selected().cloned().into_iter().collect();
        shown.view.show(GithubShown { workspaces, closely, references: Vec::new() });
    }

    /// `workspace`'s pull request, as the hub sent it, or, from a hub too old to send it, as its record has it. `None`
    /// until the hub has looked.
    pub(crate) fn pull_request_of(&self, workspace: &Workspace) -> Option<PullRequestFound> {
        self.shown_of_github.as_ref()?.view.feed().pull_request(workspace)
    }

    fn pull_requests_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.selected_pane() {
            pane.update(cx, WorkspacePane::take_pull_request);
        }
        cx.notify();
    }
}
