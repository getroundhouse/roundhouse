//! The window: the sidebar on the left and the selected workspace beside it. Shows the hub's records,
//! which client-core keeps current, and is the only place that connects and reconnects.

use std::collections::HashMap;

use client_core::records::Record;
use client_core::{Client, ClientError, HubAdmin, RecordsFeed};
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
    /// Held so the task stops when the view goes away.
    feed_task: Option<Task<()>>,
}

impl AppView {
    pub fn new(hub: HubSource, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut this = Self {
            hub,
            connection: Connection::Connecting,
            feed: None,
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
            feed_task: None,
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
        self.feed_task = Some(cx.spawn(async move |this, cx| {
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

    /// Connects, and has client-core follow the hub's records. Comes back here if the hub goes away,
    /// which starts a new one.
    pub fn connect(&mut self, cx: &mut Context<Self>) {
        self.connection = Connection::Connecting;
        self.feed = None;
        self.panes.clear();
        cx.notify();
        let hub = self.hub.clone();
        let socket = hub.paths.socket.clone();
        self.feed_task = Some(cx.spawn(async move |this, cx| {
            let connected = cx
                .background_spawn(async move {
                    let client = hub.connect()?;
                    let feed = RecordsFeed::follow(&client)?;
                    Ok::<_, ClientError>((client, feed))
                })
                .await;
            let (client, feed) = match connected {
                Ok(connected) => connected,
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
            let _ = this.update(cx, |t, cx| {
                t.connection = Connection::Ready(client);
                t.feed = Some(feed.clone());
                t.refresh(&feed, true, cx);
            });
            while feed.changed.recv_async().await.is_ok() {
                if this.update(cx, |t, cx| t.refresh(&feed, false, cx)).is_err() {
                    return;
                }
            }
            // The feed ends when the connection does.
            let _ = this.update(cx, AppView::connect);
        }));
    }

    /// Passes on records that replies carried, so what's shown needn't wait for the feed.
    pub(crate) fn take_replies(&mut self, records: impl IntoIterator<Item = Record>, cx: &mut Context<Self>) {
        let Some(feed) = self.feed.clone() else { return };
        for record in records {
            feed.take_reply(record);
        }
        // Even if client-core had these already: what's shown here may not have caught up with it yet.
        self.refresh(&feed, false, cx);
    }

    /// Brings what's shown up to date with client-core's copy of the records, and tells each pane what
    /// it holds for its workspace. `connected` is the first time on a connection.
    fn refresh(&mut self, feed: &RecordsFeed, connected: bool, cx: &mut Context<Self>) {
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
        if connected || !still_there || !has_pane {
            self.show_selected(cx);
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
            let comments =
                self.feed.as_ref().and_then(|feed| feed.read(|r| r.comments().remove(&id))).unwrap_or_default();
            let pane = cx.new(|cx| WorkspacePane::new(client, workspace, sessions, comments, window, cx));
            cx.subscribe(&pane, |t, _, event: &PaneEvent, cx| match event.clone() {
                PaneEvent::Error(message) => t.show_error(message, cx),
                PaneEvent::Disconnected => t.connect(cx),
                PaneEvent::Archive(id, force) => t.archive(id, force, cx),
                PaneEvent::Replied(record) => t.take_replies([*record], cx),
            })
            .detach();
            self.panes.insert(id.clone(), pane);
        }
        if let Some(pane) = self.panes.get(&id) {
            pane.update(cx, |p, cx| p.focus_composer(window, cx));
        }
        cx.notify();
    }

    fn selected_pane(&self) -> Option<Entity<WorkspacePane>> {
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
