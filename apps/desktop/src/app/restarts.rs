//! Restarting the hub, across a real break or to finish an update, and the app, to install one. Each can lose
//! something, agents that stop or what's typed and not sent, so each says what first.

use gpui_kit::{App, AppContext as _, Context, PromptButton, PromptLevel, Window};

use super::{AppView, Connection, view};

impl AppView {
    /// For a hub across a real break, or an older one someone chose to restart to finish an update, so one that matches
    /// this app can start. This also stops its agents, which the app says first. Only the hub with this `pid`, the one
    /// the app asked about, so one that has started since stays.
    pub fn restart_hub(&mut self, pid: u32, cx: &mut Context<Self>) {
        self.connection = Connection::Connecting;
        cx.notify();
        let socket = self.hub.paths.socket.clone();
        self.connecting = Some(cx.spawn(async move |this, cx| {
            let stopped = cx.background_spawn(async move { client_core::stop_hub_if_still(&socket, pid) }).await;
            let _ = this.update(cx, |t, cx| match stopped {
                Ok(()) => t.connect(cx),
                Err(e) => {
                    t.connection = Connection::Failed(e.to_string());
                    cx.notify();
                }
            });
        }));
    }

    /// Asks whether to finish an update now by restarting the older hub, saying what that would stop, from a status
    /// read now rather than when it connected.
    pub fn offer_hub_restart(&mut self, cx: &mut Context<Self>) {
        let (Some(client), Some(pid), None) = (self.client(), self.restartable_hub, &self.asking) else { return };
        self.asking = Some(cx.spawn(async move |this, cx| {
            let status = cx.background_spawn(async move { client.hub_status() }).await;
            let answer = this.update(cx, |t, cx| match status {
                Ok(status) => {
                    let detail = view::finish_question(&status);
                    let answers = [PromptButton::cancel("Not now"), PromptButton::new("Finish now")];
                    let ask = |window: &mut Window, cx: &mut App| {
                        window.prompt(PromptLevel::Warning, "Finish the update now?", Some(&detail), &answers, cx)
                    };
                    t.window.update(cx, |_, window, cx| ask(window, cx)).ok()
                }
                Err(e) => {
                    t.show_error(format!("couldn't see what's running on the hub: {e}"), cx);
                    None
                }
            });
            let finish = match answer {
                Ok(Some(answer)) => answer.await == Ok(1),
                _ => false,
            };
            let _ = this.update(cx, |t, cx| {
                t.asking = None;
                // Only the hub it asked about, and only while connected to it: the connection may have gone, and
                // another hub started, while it asked.
                if finish && t.client().is_some() && t.restartable_hub == Some(pid) {
                    t.restart_hub(pid, cx);
                }
            });
        }));
    }

    /// Restarts the app to install the update that's downloaded, asking first when that would lose what's typed and
    /// not sent.
    pub fn restart_to_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (chats, comments) =
            self.panes.values().map(|pane| pane.read(cx).unsent(cx)).fold((0, 0), |(a, b), (c, d)| (a + c, b + d));
        let Some(lost) = view::unsent_note(chats, comments) else {
            return crate::updates::restart_to_update(cx);
        };
        let answers = [PromptButton::cancel("Keep working"), PromptButton::new("Restart")];
        let answer = window.prompt(PromptLevel::Warning, "Restart to update?", Some(&lost), &answers, cx);
        cx.spawn(async move |_, cx| {
            if answer.await == Ok(1) {
                cx.update(crate::updates::restart_to_update);
            }
        })
        .detach();
    }
}
