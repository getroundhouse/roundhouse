//! Keeps the pull request on screen current. It changes on GitHub while nobody's looking: checks finish, reviews come
//! in, and the merge queue merges it.

use gpui_kit::{Context, Task};

use super::AppView;
use crate::pull_request::READ_AGAIN;
use crate::workspace_pane::WorkspacePane;

impl AppView {
    /// Reads the pull request of the workspace showing every [`READ_AGAIN`], unless it's merged or closed. Only that
    /// one, since the others are read again when they're shown.
    pub(super) fn watch_pull_requests(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(READ_AGAIN).await;
                let looked = this.update(cx, |t, cx| {
                    if let Some(pane) = t.selected_pane() {
                        pane.update(cx, WorkspacePane::look_again_at_pull_request);
                    }
                });
                if looked.is_err() {
                    return;
                }
            }
        })
    }
}
