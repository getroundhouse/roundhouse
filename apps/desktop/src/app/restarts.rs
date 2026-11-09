//! Restarting the hub, across a real break or to finish an update, and the app, to install one. Each can lose
//! something, agents that stop or what's typed and not sent, so each says what first.

use std::time::Duration;

use gpui_kit::{App, AppContext as _, Context, PromptButton, PromptLevel, Task, Window};

use super::{AppView, Connection, hub_source, view};

/// How often to look whether an older hub has gone idle, for what the records don't show, such as another client
/// leaving it.
pub(super) const LOOK_AGAIN: Duration = Duration::from_secs(30);

impl AppView {
    /// Finishes an update by itself, by restarting the older hub once nothing is running on it and nothing else is
    /// using it, as when the app connects. Only while the window isn't in front, since the restart swaps what's on
    /// screen for a moment, and that would interrupt someone using it. Only the hub knows about other clients and setup
    /// scripts, so this asks it.
    pub(super) fn finish_update_when_idle(&mut self, cx: &mut Context<Self>) {
        let (Some(client), Some(pid), None) = (self.client(), self.restartable_hub, &self.looking) else { return };
        let build = self.hub.build_number;
        self.looking = Some(cx.spawn(async move |this, cx| {
            let status = cx.background_spawn(async move { client.hub_status() }).await;
            let _ = this.update(cx, |t, cx| {
                t.looking = None;
                let idle = status.is_ok_and(|s| s.pid == pid && hub_source::should_replace(&s, build));
                // Not while someone is being asked whether to finish it now, or is back in the window.
                if idle && t.asking.is_none() && !t.in_front(cx) && t.still_finishing(pid) {
                    t.restart_hub(pid, cx);
                }
            });
        }));
    }

    /// Whether the window is the one in front: the key window, which another app takes when it comes in front, unlike
    /// the main window. Yes when it can't tell, so finishing waits.
    fn in_front(&self, cx: &mut App) -> bool {
        self.window.update(cx, |_, window, _| window.is_window_active()).unwrap_or(true)
    }

    /// Whether the app is still connected to the older hub with this `pid`: the connection may have gone, and another
    /// hub started, while it asked or looked.
    fn still_finishing(&self, pid: u32) -> bool {
        self.client().is_some() && self.restartable_hub == Some(pid)
    }

    /// Looks every [`LOOK_AGAIN`] for whether the older hub has gone idle.
    pub(super) fn keep_finishing(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(LOOK_AGAIN).await;
                if this.update(cx, Self::finish_update_when_idle).is_err() {
                    return;
                }
            }
        })
    }

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
                if finish && t.still_finishing(pid) {
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
