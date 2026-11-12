//! Keeping what's typed and not sent on the hub, on whichever connection there is, and showing in each workspace what's
//! typed elsewhere, such as in another client, as the hub's records tell of it.

use client_core::{Client, Drafts};
use gpui_kit::Context;

use super::{AppView, quitting};
use crate::workspace_pane::WorkspacePane;

impl AppView {
    /// Keeps drafts on `client`'s hub from here on, carrying on with any the last connection couldn't, and has quitting
    /// wait for them.
    pub(super) fn keep_drafts_on(&mut self, client: &Client, cx: &mut Context<Self>) {
        let drafts = if let Some(drafts) = &self.drafts {
            drafts.reconnect(client);
            drafts.clone()
        } else {
            let drafts = Drafts::start(client);
            follow_drafts(&drafts, cx);
            self.drafts.insert(drafts).clone()
        };
        quitting::wait_for(client, drafts, cx);
    }
}

/// Has each pane, and the new-workspace dialog, show a draft made elsewhere that it held back while one from here was
/// on its way, once that one has reached the hub.
fn follow_drafts(drafts: &Drafts, cx: &mut Context<AppView>) {
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
    .detach();
}
