//! The window: the sidebar on the left and the selected workspace beside it. Holds the hub's records,
//! kept current by its feed, and is the only place that connects and reconnects.

use std::collections::HashMap;

use client_core::{Client, ClientError, HubAdmin, SubscriptionItem};
use gpui_kit::{AnyWindowHandle, AppContext, Context, Entity, FocusHandle, Result, Task, Window};
use protocol::client::{HubNotice, HubStatus};
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

struct Snapshot {
    repos: Vec<Repo>,
    workspaces: Vec<Workspace>,
    sessions: HashMap<WorkspaceId, Vec<Session>>,
}

fn load_snapshot(client: &Client) -> Result<Snapshot, ClientError> {
    let repos = client.repo_list()?;
    let workspaces = client.workspace_list(true)?;
    let mut sessions = HashMap::new();
    for workspace in workspaces.iter().filter(|w| w.status == WorkspaceStatus::Ready) {
        sessions.insert(workspace.id.clone(), client.session_list(&workspace.id)?);
    }
    Ok(Snapshot { repos, workspaces, sessions })
}

pub struct AppView {
    hub: HubSource,
    pub connection: Connection,
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

    /// Connects and follows the feed. Comes back here if the hub goes away, which starts a new one.
    pub fn connect(&mut self, cx: &mut Context<Self>) {
        self.connection = Connection::Connecting;
        self.panes.clear();
        cx.notify();
        let hub = self.hub.clone();
        let socket = hub.paths.socket.clone();
        self.feed_task = Some(cx.spawn(async move |this, cx| {
            let connected = cx
                .background_spawn(async move {
                    let client = hub.connect()?;
                    // Subscribing first means nothing can change between the list and the feed.
                    let feed = client.feed_subscribe()?;
                    let snapshot = load_snapshot(&client)?;
                    Ok::<_, ClientError>((client, feed, snapshot))
                })
                .await;
            let (client, feed, snapshot) = match connected {
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
                t.apply_snapshot(snapshot, cx);
            });
            while let Ok(item) = feed.items.recv_async().await {
                match item {
                    SubscriptionItem::Notice(notice) => {
                        if this.update(cx, |t, cx| t.apply_notice(notice, cx)).is_err() {
                            return;
                        }
                    }
                    SubscriptionItem::Disconnected => {
                        let _ = this.update(cx, AppView::connect);
                        return;
                    }
                    _ => {}
                }
            }
        }));
    }

    fn apply_snapshot(&mut self, snapshot: Snapshot, cx: &mut Context<Self>) {
        self.repos = snapshot.repos;
        self.workspaces = snapshot.workspaces;
        self.sessions = snapshot.sessions;
        for (id, pane) in &self.panes {
            let sessions = self.sessions.get(id).cloned().unwrap_or_default();
            pane.update(cx, |p, cx| p.set_sessions(sessions, cx));
        }
        let still_there =
            self.selected.as_ref().is_some_and(|id| self.visible_workspaces().iter().any(|w| &w.id == id));
        if !still_there {
            self.selected = self.visible_workspaces().first().map(|w| w.id.clone());
        }
        self.show_selected(cx);
        cx.notify();
    }

    fn apply_notice(&mut self, notice: HubNotice, cx: &mut Context<Self>) {
        match notice {
            HubNotice::Repo(repo) => match self.repos.iter_mut().find(|r| r.id == repo.id) {
                Some(existing) => *existing = repo,
                None => self.repos.push(repo),
            },
            HubNotice::Workspace(workspace) => {
                if workspace.status == WorkspaceStatus::Archived {
                    self.panes.remove(&workspace.id);
                }
                if let Some(pane) = self.panes.get(&workspace.id) {
                    pane.update(cx, |p, cx| p.set_workspace(workspace.clone(), cx));
                }
                match self.workspaces.iter_mut().find(|w| w.id == workspace.id) {
                    Some(existing) => *existing = workspace,
                    None => self.workspaces.push(workspace),
                }
                self.keep_selection_valid(cx);
            }
            HubNotice::Session(session) => {
                if let Some(pane) = self.panes.get(&session.workspace_id) {
                    pane.update(cx, |p, cx| p.set_session(session.clone(), cx));
                }
                let list = self.sessions.entry(session.workspace_id.clone()).or_default();
                match list.iter_mut().find(|s| s.id == session.id) {
                    Some(existing) => *existing = session,
                    None => list.push(session),
                }
            }
            HubNotice::Resync => {
                if let Some(client) = self.client() {
                    cx.spawn(async move |this, cx| {
                        let snapshot = cx.background_spawn(async move { load_snapshot(&client) }).await;
                        let _ = this.update(cx, |t, cx| match snapshot {
                            Ok(snapshot) => t.apply_snapshot(snapshot, cx),
                            Err(e) => t.show_error(format!("{e}"), cx),
                        });
                    })
                    .detach();
                }
            }
            HubNotice::Unknown => {}
        }
        cx.notify();
    }

    fn keep_selection_valid(&mut self, cx: &mut Context<Self>) {
        let visible = self.visible_workspaces();
        let fine = self.selected.as_ref().is_some_and(|id| visible.iter().any(|w| &w.id == id));
        if !fine {
            self.selected = visible.first().map(|w| w.id.clone());
            self.show_selected(cx);
        }
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
            let pane = cx.new(|cx| WorkspacePane::new(client, workspace, sessions, window, cx));
            cx.subscribe(&pane, |t, _, event: &PaneEvent, cx| match event.clone() {
                PaneEvent::Error(message) => t.show_error(message, cx),
                PaneEvent::Disconnected => t.connect(cx),
                PaneEvent::Archive(id, force) => t.archive(id, force, cx),
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
