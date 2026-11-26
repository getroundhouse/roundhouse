//! Drawing how client-core's link to the hub stands, and carrying everything open on to each connection it makes. The
//! link connects, and connects again whenever the connection goes, as `client_core::link` says, with the way
//! [`HubSource`](super::HubSource) gives it, which starts this machine's hub when there's none.

use client_core::Client;
use client_core::link::LinkState;
use gpui_kit::Context;
use protocol::client::HubStatus;

use super::{AppView, Connection, ConnectionFailure, Unconnected, hub_source, restarts};

impl AppView {
    /// Listens to the link, which tells of each change: how it stands, a new connection, the records, or an error.
    pub(super) fn follow_link(&mut self, cx: &mut Context<Self>) {
        let changed = self.link.changed().clone();
        self.following = Some(cx.spawn(async move |this, cx| {
            while changed.recv_async().await.is_ok() {
                if this.update(cx, Self::link_changed).is_err() {
                    return;
                }
            }
        }));
    }

    /// Connects again now, as Try again asks. Everything already shown carries on on the new connection.
    pub fn connect(&mut self, cx: &mut Context<Self>) {
        self.show_connecting(cx);
        self.link.connect_now();
    }

    /// Connects again now, in the way [`AppView::hub`] says from now on, such as with the token pairing again renewed.
    pub(crate) fn connect_anew(&mut self, cx: &mut Context<Self>) {
        self.show_connecting(cx);
        self.link.connect_with(self.hub.clone());
    }

    pub(super) fn link_changed(&mut self, cx: &mut Context<Self>) {
        if let Some((client, status)) = self.link.take_connection() {
            self.connected(&client, status.as_ref(), cx);
        }
        let state = self.link.state();
        if state.connected().is_none() {
            self.forget_the_hub(cx);
        }
        if failed_to_start(&state) {
            crate::updates::keep_without_the_hub(self.hub.build_version, cx);
        }
        match shown(state) {
            Some(Connection::Ready(client)) => {
                self.connection = Connection::Ready(client);
                self.tell_tool_sign_in(cx);
            }
            Some(connection) if !same(&self.connection, &connection) => {
                // The hub said its status, so the app has heard which release it's on. One newer than the app, which
                // the app needs to update to talk to, would otherwise hold back every update that says the oldest
                // release it updates from.
                if let Connection::AcrossABreak(status) = &connection {
                    self.keep_workers_of(status, cx);
                }
                self.set_connection(connection, cx);
                cx.notify();
            }
            Some(_) | None => {}
        }
        if !matches!(self.connection, Connection::Ready(_)) {
            return;
        }
        if let Some(error) = self.link.take_errors().first() {
            self.show_error(error.to_string(), cx);
        }
        if let Some(feed) = self.feed() {
            self.refresh(&feed, cx);
        }
    }

    /// Carries everything open on to `client`, a new connection, which is ready once client-core has caught up on it,
    /// so the sidebar never shows empty. `status` is the hub's, as it connected, for this machine's hub.
    fn connected(&mut self, client: &Client, status: Option<&HubStatus>, cx: &mut Context<Self>) {
        self.forget_the_hub(cx);
        // The link's changes gather into one, so the app may not have seen the last connection go. Until this one has
        // caught up, nothing is sent on either, and each pane holds what it would send.
        if matches!(self.connection, Connection::Ready(_)) {
            self.set_connection(Connection::Connecting, cx);
        }
        // Of the hub before, if this is another, or of this one before it restarted on the release it was asked for.
        self.hub_update_asked = None;
        self.keep_drafts_on(client, cx);
        if self.shown.is_none()
            && let Some(feed) = self.feed()
        {
            self.start_showing(&feed);
        }
        self.conversations.update(cx, |conversations, _| conversations.connected(client));
        // Every pane stays as it was, with whatever's typed in it, and carries on here.
        for pane in self.panes.values() {
            pane.update(cx, |p, cx| p.reconnect(client, cx));
        }
        self.reconnect_dialog(client, cx);
        let Some(status) = status else { return };
        self.keep_workers_of(status, cx);
        let build = self.hub.build_number;
        let restarting = hub_source::restarts_at_once(status, build);
        let older = restarts::OlderHub { pid: status.pid, restarting };
        self.restartable_hub = hub_source::may_restart(status, build).then_some(older);
        self.finishing = self.restartable_hub.map(|_| Self::keep_finishing(cx));
    }

    /// Forgets what it knew of the hub it was connected to, which says nothing about the one it connects to next.
    fn forget_the_hub(&mut self, cx: &mut Context<Self>) {
        (self.restartable_hub, self.finishing, self.looking) = (None, None, None);
        self.drop_restart_question(cx);
    }
}

/// What the window shows of how the link stands, or `None` to go on showing what it does while a new connection catches
/// up. A hub the link tries again by itself goes on showing why it failed, so it doesn't flash between that and
/// Connecting.
fn shown(state: LinkState<Unconnected>) -> Option<Connection> {
    match state {
        LinkState::Connecting(None) => Some(Connection::Connecting),
        LinkState::CatchingUp(_) => None,
        LinkState::Ready(client) => Some(Connection::Ready(client)),
        LinkState::Connecting(Some(unconnected))
        | LinkState::Waiting(unconnected)
        | LinkState::Stopped(unconnected) => Some(match unconnected {
            Unconnected::Failed(failure) => Connection::Failed(failure),
            Unconnected::AcrossABreak(status) => Connection::AcrossABreak(status),
        }),
    }
}

/// Whether this computer's hub failed to start, or couldn't be reached, in a way that waits for a person to try again.
/// A paired hub's failure says nothing of this computer's hub, and a connection that went is tried again by itself.
fn failed_to_start(state: &LinkState<Unconnected>) -> bool {
    matches!(state, LinkState::Stopped(Unconnected::Failed(ConnectionFailure::Local(_))))
}

/// Whether `next` shows as `shown` does, so the panes needn't be told again.
fn same(shown: &Connection, next: &Connection) -> bool {
    match (shown, next) {
        (Connection::Connecting, Connection::Connecting) => true,
        (Connection::Failed(was), Connection::Failed(is)) => was == is,
        (Connection::AcrossABreak(was), Connection::AcrossABreak(is)) => was == is,
        (Connection::Connecting | Connection::Ready(_) | Connection::Failed(_) | Connection::AcrossABreak(_), _) => {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hub tried again by itself, and one caught up on, keep showing what they did, and a try that was asked for shows
    /// Connecting.
    #[test]
    fn a_hub_tried_again_by_itself_keeps_showing_why_it_failed() {
        let unreachable = || Unconnected::Failed(ConnectionFailure::Unreachable { hosts: vec!["127.0.0.1:1".into()] });
        let failed = |shown: Option<Connection>| {
            matches!(shown, Some(Connection::Failed(ConnectionFailure::Unreachable { .. })))
        };

        assert!(failed(shown(LinkState::Connecting(Some(unreachable())))), "trying again");
        assert!(failed(shown(LinkState::Waiting(unreachable()))), "waiting to");
        assert!(failed(shown(LinkState::Stopped(unreachable()))), "stopped");
        assert!(matches!(shown(LinkState::Connecting(None)), Some(Connection::Connecting)), "asked to try");
    }

    /// Only then does the app's update go by the app's own release alone.
    #[test]
    fn only_this_computer_s_hub_stopped_for_a_person_failed_to_start() {
        let local = || Unconnected::Failed(ConnectionFailure::Local("couldn't start the hub".into()));
        let paired = || Unconnected::Failed(ConnectionFailure::Unreachable { hosts: vec!["127.0.0.1:1".into()] });

        assert!(failed_to_start(&LinkState::Stopped(local())));
        assert!(!failed_to_start(&LinkState::Waiting(local())), "a connection that went, tried again by itself");
        assert!(!failed_to_start(&LinkState::Connecting(Some(local()))), "trying again");
        assert!(!failed_to_start(&LinkState::Stopped(paired())), "a paired hub");
        assert!(!failed_to_start(&LinkState::Stopped(Unconnected::Failed(ConnectionFailure::Revoked))), "a paired hub");
    }
}
