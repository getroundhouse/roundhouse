//! Restarting the hub, across a real break or to finish an update, and the app, to install one. Each can lose
//! something, agents that stop or what's typed and not sent, so each says what first. Restarting the hub now asks in a
//! dialog, since it stops agents, and restarting the app in the card at the bottom of the sidebar.

use std::path::Path;
use std::time::Duration;

use gpui_kit::component::WindowExt as _;
use gpui_kit::{App, AppContext as _, Context, Task};
use protocol::client::HubStatus;

use super::{AppView, Connection, hub_source, view};
use crate::workspace_pane::WorkspacePane;

mod asking;

/// What the app asks before going ahead with something about an update that loses work: in the update's card before
/// restarting the app, and in a dialog before restarting the older hub. More can be typed, or start running, while it
/// asks, so going ahead first checks it loses no more than it said.
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
            Self::Finish(_) => "Restart now?",
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
    /// using it, as when the app connects. Whenever that is, even while someone is using the app: the restart stops
    /// nothing, and the workspace stays on screen through it, while waiting for a better moment left the update
    /// looking stuck.
    /// Only the hub knows about other clients and setup scripts, so this asks it, and the hub has the last word as
    /// it stops, since a prompt can be sent between this seeing it idle and stopping it.
    pub(super) fn finish_update_when_idle(&mut self, cx: &mut Context<Self>) {
        let (Some(client), Some(pid), None) = (self.client(), self.restartable_hub, &self.looking) else { return };
        let build = self.hub.build_number;
        self.looking = Some(cx.spawn(async move |this, cx| {
            let status = cx.background_spawn(async move { client.hub_status() }).await;
            let _ = this.update(cx, |t, cx| {
                t.looking = None;
                let idle = status.is_ok_and(|s| s.pid == pid && hub_source::should_replace(&s, build));
                // The dialog asking whether to restart now doesn't hold it up, since finishing an idle hub stops
                // nothing, and the dialog goes with the hub.
                if idle && t.still_finishing(pid) {
                    t.restart_hub_if_idle(pid, cx);
                }
            });
        }));
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
        self.stop_then_connect(cx, move |socket| client_core::stop_hub_if_still(socket, pid));
    }

    /// For an older hub that looked idle, which this says stops nothing, so the hub is asked to stop only if it still
    /// is. One that's busy after all is connected to again, and the update goes on waiting.
    fn restart_hub_if_idle(&mut self, pid: u32, cx: &mut Context<Self>) {
        let Some(client) = self.client() else { return };
        self.stop_then_connect(cx, move |socket| client_core::stop_hub_if_idle(&client, socket, pid).map(drop));
    }

    /// Has the app be connecting. A workspace already on screen stays there, as `view.rs` draws it, so each is told to
    /// hold what it would send until the hub is back.
    pub(super) fn show_connecting(&mut self, cx: &mut Context<Self>) {
        self.connection = Connection::Connecting;
        for pane in self.panes.values() {
            pane.update(cx, WorkspacePane::hub_went_away);
        }
        cx.notify();
    }

    /// Shows the app as connecting while `stop` stops the hub at the socket it's given, then connects, which starts
    /// one that matches this app if that hub has gone.
    fn stop_then_connect(
        &mut self,
        cx: &mut Context<Self>,
        stop: impl FnOnce(&Path) -> client_core::Result<()> + Send + 'static,
    ) {
        // Whatever was being asked about this hub is settled, including when stopping it fails.
        self.drop_restart_question(cx);
        self.show_connecting(cx);
        let socket = self.hub.paths.socket.clone();
        self.connecting = Some(cx.spawn(async move |this, cx| {
            let stopped = cx.background_spawn(async move { stop(&socket) }).await;
            let _ = this.update(cx, |t, cx| match stopped {
                Ok(()) => t.connect(cx),
                Err(e) => {
                    t.connection = Connection::Failed(e.to_string());
                    cx.notify();
                }
            });
        }));
    }

    /// Finishes an update now by restarting the older hub, as `Restart now` asks, after asking in a dialog that says
    /// what that would stop, from a status read now rather than when it connected. With nothing to stop there's
    /// nothing to ask, so it goes ahead.
    pub fn offer_hub_restart(&mut self, cx: &mut Context<Self>) {
        let build = self.hub.build_number;
        self.read_the_older_hub(cx, move |t, status, cx| {
            if hub_source::should_replace(&status, build) {
                t.restart_hub_if_idle(status.pid, cx);
            } else {
                t.ask_to_restart_now(status, cx);
            }
        });
    }

    /// Opens the dialog that asks, and keeps what it asked for as long as it's open.
    fn ask_to_restart_now(&mut self, status: HubStatus, cx: &mut Context<Self>) {
        let question = UpdateQuestion::Finish(status.clone());
        let (restart, closed) = (cx.weak_entity(), cx.weak_entity());
        let opened = self.window.update(cx, |_, window, cx| {
            asking::open(
                &question,
                window,
                cx,
                move |cx| drop(restart.update(cx, |t, cx| t.finish_update_now(&status, cx))),
                move |cx| drop(closed.update(cx, Self::dismiss_update_question)),
            );
        });
        if opened.is_ok() {
            self.ask(question, cx);
        }
    }

    /// Forgets what it asked about the hub it was connected to, and closes the dialog asking it, which goes with that
    /// hub. Closing a dialog from here doesn't tell this view, as someone closing it does, so this forgets by itself.
    pub(super) fn drop_restart_question(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.update_question, Some(UpdateQuestion::Finish(_))) {
            return;
        }
        self.update_question = None;
        // Once this update is over, since this can be in the middle of updating the window, which can't be asked then.
        let window = self.window;
        cx.defer(move |cx| drop(window.update(cx, |_, window, cx| window.close_dialog(cx))));
    }

    /// Finishes the update now, as the dialog asked when it `said` what that stops, unless more is running on the
    /// older hub than that, which it asks about instead.
    pub fn finish_update_now(&mut self, said: &HubStatus, cx: &mut Context<Self>) {
        let (agents, setups) = (said.running_sessions, said.running_setups);
        self.read_the_older_hub(cx, move |t, now, cx| {
            if now.running_sessions > agents || now.running_setups > setups {
                return t.ask_to_restart_now(now, cx);
            }
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

    /// How many chats and comments have something typed in them that isn't sent. What's left in the new-workspace
    /// dialog counts as a chat's, since it's for a workspace's first chat.
    fn unsent(&self, cx: &App) -> (usize, usize) {
        let dialog = self.new_workspace.as_ref().is_some_and(|dialog| dialog.read(cx).has_anything(cx));
        let panes = self.panes.values().map(|pane| pane.read(cx).unsent(cx));
        panes.fold((usize::from(dialog), 0), |(a, b), (c, d)| (a + c, b + d))
    }
}
