//! Restarting the hub, across a real break or to finish an update, and the app, to install one. Each can lose
//! something, agents that stop or what's typed and not sent, so each says what first. For an update, the card at the
//! bottom of the sidebar asks, rather than a dialog.

use std::time::Duration;

use gpui_kit::{App, AppContext as _, Context, Task};
use protocol::client::HubStatus;

use super::{AppView, Connection, hub_source, view};

/// What the update's card asks before going ahead with something that loses work. It holds nothing else in the app
/// up, so more can be typed, or start running, while it asks, and going ahead first checks it loses no more than it
/// said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateQuestion {
    /// Whether to restart to update, which loses what's typed and not sent in this many chats and comments.
    Restart { chats: usize, comments: usize },
    /// Whether to finish the update by restarting the older hub, which stops what this says is running on it.
    Finish(HubStatus),
}

impl UpdateQuestion {
    pub fn title(&self) -> &'static str {
        match self {
            Self::Restart { .. } => "Restart to update?",
            Self::Finish(_) => "Finish the update now?",
        }
    }

    /// What going ahead loses.
    pub fn detail(&self) -> String {
        match self {
            Self::Restart { chats, comments } => view::unsent_note(*chats, *comments).unwrap_or_default(),
            Self::Finish(status) => view::finish_question(status),
        }
    }
}

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
                // Not while someone is back in the window, which it takes as so when it can't tell, so finishing waits.
                // The card asking whether to finish it now doesn't hold it up, since finishing an idle hub stops
                // nothing, and the question goes with the card.
                if idle && !t.in_front(cx).unwrap_or(true) && t.still_finishing(pid) {
                    t.restart_hub(pid, cx);
                }
            });
        }));
    }

    /// Whether the window is the one in front: the key window, which another app takes when it comes in front, unlike
    /// the main window. `None` when it can't tell, such as in the middle of updating the window.
    pub(super) fn in_front(&self, cx: &mut App) -> Option<bool> {
        self.window.update(cx, |_, window, _| window.is_window_active()).ok()
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
        self.read_the_older_hub(cx, |t, status, cx| t.ask(UpdateQuestion::Finish(status), cx));
    }

    /// Finishes the update now, as the card asked, unless more is running on the older hub than it said, which it
    /// asks about instead.
    pub fn finish_update_now(&mut self, cx: &mut Context<Self>) {
        let Some(UpdateQuestion::Finish(said)) = self.update_question.clone() else { return };
        self.read_the_older_hub(cx, move |t, now, cx| {
            if now.running_sessions > said.running_sessions || now.running_setups > said.running_setups {
                return t.ask(UpdateQuestion::Finish(now), cx);
            }
            t.update_question = None;
            t.restart_hub(now.pid, cx);
        });
    }

    /// Reads what's running on the older hub the app may restart, and then does `then` with it, if the app is still
    /// connected to that hub.
    fn read_the_older_hub(
        &mut self,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, HubStatus, &mut Context<Self>) + 'static,
    ) {
        let (Some(client), Some(pid), None) = (self.client(), self.restartable_hub, &self.asking) else { return };
        self.asking = Some(cx.spawn(async move |this, cx| {
            let status = cx.background_spawn(async move { client.hub_status() }).await;
            let _ = this.update(cx, |t, cx| {
                t.asking = None;
                match status {
                    Ok(status) if status.pid == pid && t.still_finishing(pid) => then(t, status, cx),
                    // Another hub since, which the app connects to rather than asks about.
                    Ok(_) => {}
                    Err(e) => t.show_error(format!("couldn't see what's running on the hub: {e}"), cx),
                }
            });
        }));
    }

    /// Restarts the app to install the update that's downloaded. When that would lose what's typed and not sent, the
    /// card asks first.
    pub fn restart_to_update(&mut self, cx: &mut Context<Self>) {
        match self.unsent(cx) {
            (0, 0) => crate::updates::restart_to_update(cx),
            (chats, comments) => self.ask(UpdateQuestion::Restart { chats, comments }, cx),
        }
    }

    /// Restarts to update, as the card asked, unless more is typed and not sent than it said, which it asks about
    /// instead.
    pub fn restart_to_update_anyway(&mut self, cx: &mut Context<Self>) {
        let Some(UpdateQuestion::Restart { chats: said_chats, comments: said_comments }) = self.update_question else {
            return;
        };
        let (chats, comments) = self.unsent(cx);
        if chats > said_chats || comments > said_comments {
            return self.ask(UpdateQuestion::Restart { chats, comments }, cx);
        }
        self.update_question = None;
        cx.notify();
        crate::updates::restart_to_update(cx);
    }

    /// Leaves things as they are, as `Not now` and `Keep working` do.
    pub fn dismiss_update_question(&mut self, cx: &mut Context<Self>) {
        self.update_question = None;
        cx.notify();
    }

    fn ask(&mut self, question: UpdateQuestion, cx: &mut Context<Self>) {
        self.update_question = Some(question);
        cx.notify();
    }

    /// How many chats and comments have something typed in them that isn't sent.
    fn unsent(&self, cx: &App) -> (usize, usize) {
        self.panes.values().map(|pane| pane.read(cx).unsent(cx)).fold((0, 0), |(a, b), (c, d)| (a + c, b + d))
    }
}
