//! The window: the sidebar on the left and the selected workspace beside it. Shows the hub's records,
//! which client-core keeps current, and is the only place that connects and reconnects.

use std::collections::HashMap;

use client_core::records::Record;
use client_core::{Client, HubAdmin, RecordsFeed};
use gpui_kit::{AnyWindowHandle, AppContext, Context, Entity, FocusHandle, Task, Window};
use protocol::client::HubStatus;
use protocol::model::{Repo, RepoId, Session, Workspace, WorkspaceId, WorkspaceStatus};

use crate::workspace_pane::{PaneEvent, WorkspacePane};

mod build_label;
mod hub_source;
mod view;
mod workspaces;

pub use build_label::BuildLabel;
pub use hub_source::HubSource;

/// The position `by` away from `at` in a list of `len`, wrapping around. `at` has to be in the list,
/// and `by` no bigger than the list.
pub(crate) fn wrapped_step(at: usize, by: isize, len: usize) -> usize {
    (at + len).wrapping_add_signed(by) % len
}

pub enum Connection {
    Connecting,
    Ready(Client),
    Failed(String),
    /// A hub is running, and there's a real protocol break between it and this app.
    AcrossABreak(HubStatus),
}

pub struct AppView {
    hub: HubSource,
    pub connection: Connection,
    /// Client-core's copy of the hub's records. The lists below are what's shown of it.
    feed: Option<RecordsFeed>,
    /// A new connection, until client-core has caught up on it.
    joining: Option<Client>,
    pub repos: Vec<Repo>,
    pub workspaces: Vec<Workspace>,
    pub sessions: HashMap<WorkspaceId, Vec<Session>>,
    pub selected: Option<WorkspaceId>,
    pub panes: HashMap<WorkspaceId, Entity<WorkspacePane>>,
    pub error: Option<String>,
    pub show_archived: bool,
    /// Workspaces being created or archived, so their buttons can't be pressed twice.
    pub busy: bool,
    /// The repository a new workspace is on its way in. It can take a while, as it fetches first.
    pub creating_in: Option<RepoId>,
    focus: FocusHandle,
    window: AnyWindowHandle,
    /// Held so these tasks stop when the view goes away.
    connecting: Option<Task<()>>,
    following: Option<Task<()>>,
}

impl AppView {
    pub fn new(hub: HubSource, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut this = Self {
            hub,
            connection: Connection::Connecting,
            feed: None,
            joining: None,
            repos: Vec::new(),
            workspaces: Vec::new(),
            sessions: HashMap::new(),
            selected: None,
            panes: HashMap::new(),
            error: None,
            show_archived: false,
            busy: false,
            creating_in: None,
            focus,
            window: window.window_handle(),
            connecting: None,
            following: None,
        };
        this.connect(cx);
        this
    }

    pub fn client(&self) -> Option<Client> {
        match &self.connection {
            Connection::Ready(client) => Some(client.clone()),
            _ => None,
        }
    }

    /// For a hub across a real break, so one that matches this app can start. This also stops its agents,
    /// which the app says first.
    pub fn restart_hub(&mut self, cx: &mut Context<Self>) {
        self.connection = Connection::Connecting;
        cx.notify();
        let socket = self.hub.paths.socket.clone();
        self.connecting = Some(cx.spawn(async move |this, cx| {
            let stopped = cx.background_spawn(async move { client_core::stop_hub(&socket) }).await;
            let _ = this.update(cx, |t, cx| match stopped {
                Ok(()) => t.connect(cx),
                Err(e) => {
                    t.connection = Connection::Failed(e.to_string());
                    cx.notify();
                }
            });
        }));
    }

    pub fn build_label(&self) -> BuildLabel {
        let client = self.client();
        BuildLabel::new(self.hub.build_version, client.as_ref().map(Client::hub_version))
    }

    pub fn show_error(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.error = Some(message.into());
        cx.notify();
    }

    /// Connects. Everything already shown carries on on the new connection, and the first time, client-core
    /// starts following the hub's records. Comes back here if the connection goes, which starts a new hub.
    pub fn connect(&mut self, cx: &mut Context<Self>) {
        self.connection = Connection::Connecting;
        cx.notify();
        let hub = self.hub.clone();
        let socket = hub.paths.socket.clone();
        self.connecting = Some(cx.spawn(async move |this, cx| {
            let connected = cx.background_spawn(async move { hub.connect() }).await;
            let client = match connected {
                Ok(client) => client,
                Err(e) => {
                    let status = if e.is_version_mismatch() {
                        cx.background_spawn(async move { HubAdmin::connect(&socket)?.status() }).await.ok()
                    } else {
                        None
                    };
                    let _ = this.update(cx, |t, cx| {
                        t.connection = match status {
                            Some(status) => Connection::AcrossABreak(status),
                            None => Connection::Failed(format!("{e}")),
                        };
                        cx.notify();
                    });
                    return;
                }
            };
            let _ = this.update(cx, |t, cx| t.connected(client, cx));
        }));
    }

    /// Ready once client-core has caught up on this connection, so the sidebar never shows empty.
    fn connected(&mut self, client: Client, cx: &mut Context<Self>) {
        if let Some(feed) = &self.feed {
            feed.reconnect(&client);
        } else {
            let feed = RecordsFeed::start(&client);
            self.follow(feed.clone(), cx);
            self.feed = Some(feed);
        }
        // Every pane stays as it was, with whatever's typed in it, and carries on here.
        for pane in self.panes.values() {
            pane.update(cx, |p, cx| p.reconnect(&client, cx));
        }
        self.joining = Some(client);
    }

    /// Shows what client-core holds whenever it changes, and connects again when its connection goes.
    fn follow(&mut self, feed: RecordsFeed, cx: &mut Context<Self>) {
        self.following = Some(cx.spawn(async move |this, cx| {
            let changed = feed.changed().clone();
            while changed.recv_async().await.is_ok() {
                if this.update(cx, |t, cx| t.feed_changed(&feed, cx)).is_err() {
                    return;
                }
            }
        }));
    }

    fn feed_changed(&mut self, feed: &RecordsFeed, cx: &mut Context<Self>) {
        let errors = feed.take_errors();
        if !feed.connected() {
            // Following failed on a connection that's there, which needs a person. A connection that went
            // is made again.
            if let Some(error) = errors.first() {
                self.joining = None;
                self.connection = Connection::Failed(error.to_string());
                cx.notify();
            } else if matches!(self.connection, Connection::Ready(_)) {
                self.connect(cx);
            }
            return;
        }
        if let Some(client) = self.joining.take() {
            self.connection = Connection::Ready(client);
        }
        if let Some(error) = errors.first() {
            self.show_error(error.to_string(), cx);
        }
        self.refresh(feed, cx);
    }

    /// Passes on a record a reply carried, so what's shown needn't wait for the feed.
    pub(crate) fn take_reply(&mut self, record: Record, cx: &mut Context<Self>) {
        let Some(feed) = self.feed.clone() else { return };
        feed.take_reply(record);
        self.refresh(&feed, cx);
    }

    /// Brings what's shown up to date with client-core's copy of the records, and tells each pane what
    /// it holds for its workspace.
    fn refresh(&mut self, feed: &RecordsFeed, cx: &mut Context<Self>) {
        let (repos, workspaces, sessions, mut comments) =
            feed.read(|records| (records.repos(), records.workspaces(), records.sessions(), records.comments()));
        self.panes.retain(|id, _| workspaces.iter().any(|w| &w.id == id && w.status != WorkspaceStatus::Archived));
        for (id, pane) in &self.panes {
            let Some(workspace) = workspaces.iter().find(|w| &w.id == id).cloned() else { continue };
            let its_sessions = sessions.get(id).cloned().unwrap_or_default();
            let its_comments = comments.remove(id).unwrap_or_default();
            pane.update(cx, |p, cx| p.set_records(workspace, its_sessions, its_comments, cx));
        }
        self.repos = repos;
        self.workspaces = workspaces;
        self.sessions = sessions;
        let still_there =
            self.selected.as_ref().is_some_and(|id| self.visible_workspaces().iter().any(|w| &w.id == id));
        if !still_there {
            self.selected = self.visible_workspaces().first().map(|w| w.id.clone());
        }
        let has_pane = self.selected.as_ref().is_some_and(|id| self.panes.contains_key(id));
        if !still_there || !has_pane {
            self.show_selected(cx);
        }
        // After a reconnect, the pane on screen catches up once the records have, and the rest when they're shown.
        if let Some(pane) = self.selected_pane() {
            pane.update(cx, WorkspacePane::catch_up);
        }
        cx.notify();
    }

    pub fn visible_workspaces(&self) -> Vec<&Workspace> {
        self.workspaces.iter().filter(|w| self.show_archived || w.status == WorkspaceStatus::Ready).collect()
    }

    pub fn select(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.show_selected(cx);
        cx.notify();
    }

    fn step_selection(&mut self, by: isize, cx: &mut Context<Self>) {
        let visible: Vec<WorkspaceId> = self.visible_workspaces().iter().map(|w| w.id.clone()).collect();
        if visible.is_empty() {
            return;
        }
        let at = self.selected.as_ref().and_then(|id| visible.iter().position(|v| v == id)).unwrap_or(0);
        let next = wrapped_step(at, by, visible.len());
        self.select(visible[next].clone(), cx);
    }

    /// Makes sure the selected workspace is on screen with its composer focused. This needs the window,
    /// and a click handler is already inside a window update, so it runs just after whatever called it.
    fn show_selected(&mut self, cx: &mut Context<Self>) {
        let (handle, this) = (self.window, cx.weak_entity());
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |t, cx| t.make_selected_pane(window, cx));
            });
        });
    }

    fn make_selected_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else { return };
        if !self.panes.contains_key(&id) {
            let (Some(client), Some(workspace)) = (self.client(), self.workspaces.iter().find(|w| w.id == id).cloned())
            else {
                return;
            };
            if workspace.status != WorkspaceStatus::Ready {
                return;
            }
            let sessions = self.sessions.get(&id).cloned().unwrap_or_default();
            let Some(records) = self.feed.clone() else { return };
            let comments = records.read(|r| r.comments().remove(&id)).unwrap_or_default();
            let pane = cx.new(|cx| WorkspacePane::new(client, records, workspace, sessions, comments, window, cx));
            cx.subscribe(&pane, |t, _, event: &PaneEvent, cx| match event.clone() {
                PaneEvent::Error(message) => t.show_error(message, cx),
                PaneEvent::Archive(id, force) => t.archive(id, force, cx),
            })
            .detach();
            self.panes.insert(id.clone(), pane);
        }
        // Only with a connection. Without one, `refresh` catches it up once there is one.
        if let Some(pane) = self.selected_pane() {
            pane.update(cx, WorkspacePane::catch_up);
        }
        if let Some(pane) = self.panes.get(&id) {
            pane.update(cx, |p, cx| p.focus_composer(window, cx));
        }
        cx.notify();
    }

    /// None without a connection, when panes are kept but neither shown nor used.
    fn selected_pane(&self) -> Option<Entity<WorkspacePane>> {
        if !matches!(self.connection, Connection::Ready(_)) {
            return None;
        }
        self.selected.as_ref().and_then(|id| self.panes.get(id).cloned())
    }

    /// The repository to put a new workspace in: the selected workspace's, or the first.
    pub fn current_repo(&self) -> Option<RepoId> {
        let selected = self.selected.as_ref().and_then(|id| self.workspaces.iter().find(|w| &w.id == id));
        selected.map(|w| w.repo_id.clone()).or_else(|| self.repos.first().map(|r| r.id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepping_wraps_around_both_ends() {
        assert_eq!(wrapped_step(0, -1, 3), 2);
        assert_eq!(wrapped_step(2, 1, 3), 0);
        assert_eq!(wrapped_step(1, 1, 3), 2);
        assert_eq!(wrapped_step(0, 1, 1), 0);
    }
}
