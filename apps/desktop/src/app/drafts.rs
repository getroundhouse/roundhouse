//! Keeping what's typed and not sent on the hub, which the hub's `Drafts` does on whichever connection there is, and
//! showing in each workspace what's typed elsewhere, such as in another client, as the hub's records tell of it.

use client_core::Drafts;
use gpui_kit::{Context, Task};

use super::AppView;
use crate::workspace_pane::WorkspacePane;

/// Has each pane, and the new-workspace dialog, show a draft made elsewhere that it held back while one from here was
/// on its way, once that one has reached the hub.
pub(super) fn follow_drafts(drafts: &Drafts, cx: &mut Context<AppView>) -> Task<()> {
    let caught_up = drafts.caught_up().clone();
    cx.spawn(async move |this, cx| {
        while caught_up.recv_async().await.is_ok() {
            let shown = this.update(cx, |t, cx| {
                for pane in t.panes.values() {
                    pane.update(cx, WorkspacePane::take_drafts_from_elsewhere);
                }
                t.show_kept_in_dialog(cx);
            });
            if shown.is_err() {
                return;
            }
        }
    })
}
