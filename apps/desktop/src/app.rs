//! The window: the sidebar on the left and the selected workspace beside it. Shows the hub's records,
//! which client-core keeps current, and is the only place that connects and reconnects.

use std::collections::{HashMap, HashSet};

use client_core::records::Record;
use client_core::{Client, HubAdmin, RecordsFeed};
use gpui_kit::component::VirtualListScrollHandle;
use gpui_kit::{AnyWindowHandle, App, AppContext, Context, Entity, FocusHandle, Task, Window};
use protocol::client::HubStatus;
use protocol::model::{Repo, Session, Workspace, WorkspaceId, WorkspaceStatus};

use crate::busy::Busy;
use crate::checkout::Checkout;
use crate::new_workspace::NewWorkspaceDialog;
use crate::workspace_pane::{PaneEvent, WorkspacePane};

mod build_line;
mod home;
mod hub_source;
mod pull_requests;
mod quitting;
mod reading;
mod restarts;
mod settings;
mod shortcuts;
mod view;
mod workspaces;

pub use build_line::{Agents, BuildLine};
pub use hub_source::HubSource;
pub use restarts::UpdateQuestion;

/// How often the app looks whether an older hub has gone idle, which a test can wait out.
#[cfg(feature = "testing")]
pub const LOOK_AGAIN: std::time::Duration = restarts::LOOK_AGAIN;

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
    pub(crate) hub: HubSource,
    /// The checkout this build is from, which the sidebar names, with why it's open.
    pub(crate) checkout: Option<Checkout>,
    pub connection: Connection,
    /// Client-core's copy of the hub's records. The lists below are what's shown of it.
    feed: Option<RecordsFeed>,
    /// A new connection, until client-core has caught up on it.
    joining: Option<Client>,
    /// The pid of the hub this is connected to, when it's an older one a client started, which this app may restart.
    /// A hub's build can't change while it runs, so this is only found when connecting.
    restartable_hub: Option<u32>,
    pub repos: Vec<Repo>,
    pub workspaces: Vec<Workspace>,
    pub sessions: HashMap<WorkspaceId, Vec<Session>>,
    pub selected: Option<WorkspaceId>,
    /// Whether the selected workspace shows, or Home in its place.
    main: home::Main,
    /// What Home shows, kept while it's put away.
    pub(crate) home: crate::home::Home,
    pub panes: HashMap<WorkspaceId, Entity<WorkspacePane>>,
    pub error: Option<String>,
    /// Where the sidebar's list is scrolled to. The list is made again each frame, so it can't keep that itself.
    pub(crate) sidebar_scroll: VirtualListScrollHandle,
    /// The workspace whose row in the sidebar the pointer is over, which shows its archive button.
    pub(crate) pointing_at: Option<WorkspaceId>,
    pub busy: Busy,
    /// The workspaces the hub is finding a new title for, whose rows and headers say so.
    pub(crate) retitling: HashSet<WorkspaceId>,
    /// The new-workspace dialog, made the first time it opens, then kept with whatever was left in it.
    pub new_workspace: Option<Entity<NewWorkspaceDialog>>,
    /// The workspace selected while a dialog was open, whose pane is focused once the dialog closes.
    focus_after_dialog: Option<WorkspaceId>,
    /// The mouse went down in the strip along the top, where no control took it, so moving it moves the window.
    moving_window: bool,
    focus: FocusHandle,
    /// Settings, when it shows in place of the sidebar and the workspace.
    settings: Option<settings::Shown>,
    /// Where keys go while Settings shows.
    settings_focus: FocusHandle,
    window: AnyWindowHandle,
    /// Held so these tasks stop when the view goes away.
    connecting: Option<Task<()>>,
    following: Option<Task<()>>,
    /// What the update's card asks before going ahead, while it asks.
    pub update_question: Option<UpdateQuestion>,
    /// Reading what's running on the older hub, to ask about finishing the update or to go ahead with it, so pressing
    /// again doesn't read it twice.
    asking: Option<Task<()>>,
    /// Looks every so often, while connected to an older hub, for whether it has gone idle, so the update can finish.
    finishing: Option<Task<()>>,
    /// Asking the older hub whether it's idle.
    looking: Option<Task<()>>,
    _watching_pull_requests: Task<()>,
    /// What the menu bar was last given to say.
    menus_say: crate::menus::Says,
}

impl AppView {
    pub fn new(hub: HubSource, checkout: Option<Checkout>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        crate::updates::observe(cx).detach();
        crate::settings::observe(cx, |_, cx| cx.notify()).detach();
        // Someone going to another app is the moment to finish an update without interrupting them, and coming back is
        // when they read the chat showing.
        cx.observe_window_activation(window, |t, window, cx| {
            t.finish_update_when_idle(cx);
            t.read_on_screen(window.is_window_active(), cx);
        })
        .detach();
        let mut this = Self {
            hub,
            checkout,
            connection: Connection::Connecting,
            feed: None,
            joining: None,
            restartable_hub: None,
            repos: Vec::new(),
            workspaces: Vec::new(),
            sessions: HashMap::new(),
            selected: None,
            main: home::Main::default(),
            home: crate::home::Home::new(window, cx),
            panes: HashMap::new(),
            error: None,
            sidebar_scroll: VirtualListScrollHandle::new(),
            pointing_at: None,
            busy: Busy::Free,
            retitling: HashSet::new(),
            new_workspace: None,
            focus_after_dialog: None,
            moving_window: false,
            focus,
            settings: None,
            settings_focus: cx.focus_handle(),
            window: window.window_handle(),
            connecting: None,
            following: None,
            update_question: None,
            asking: None,
            finishing: None,
            looking: None,
            _watching_pull_requests: Self::watch_pull_requests(cx),
            menus_say: crate::menus::Says::default(),
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

    pub fn build_line(&self, cx: &App) -> BuildLine {
        let client = self.client();
        let hub = client.as_ref().map(Client::hub_version);
        // As the hub counts them, from what the sidebar already has, so it's current without asking.
        let agents = Agents::of(self.sessions.values().flatten().map(|s| s.status));
        BuildLine::new(self.hub.build_version, hub, self.restartable_hub.is_some(), agents, crate::updates::ready(cx))
    }

    pub fn show_error(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.error = Some(message.into());
        cx.notify();
    }

    /// Connects. Everything already shown carries on on the new connection, and the first time, client-core
    /// starts following the hub's records. Comes back here if the connection goes, which starts a new hub.
    pub fn connect(&mut self, cx: &mut Context<Self>) {
        self.connection = Connection::Connecting;
        // What it was connected to says nothing about what it connects to next.
        (self.restartable_hub, self.finishing, self.looking) = (None, None, None);
        if matches!(self.update_question, Some(UpdateQuestion::Finish(_))) {
            self.update_question = None;
        }
        cx.notify();
        let hub = self.hub.clone();
        let socket = hub.paths.socket.clone();
        self.connecting = Some(cx.spawn(async move |this, cx| {
            let connected = cx.background_spawn(async move { hub.connect() }).await;
            let (client, status) = match connected {
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
            let _ = this.update(cx, |t, cx| t.connected(client, &status, cx));
        }));
    }

    /// Ready once client-core has caught up on this connection, so the sidebar never shows empty.
    fn connected(&mut self, client: Client, status: &HubStatus, cx: &mut Context<Self>) {
        quitting::wait_for(&client, cx);
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
        self.restartable_hub = hub_source::may_restart(status, self.hub.build_number).then_some(status.pid);
        self.finishing = self.restartable_hub.map(|_| Self::keep_finishing(cx));
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
        // A new connection, or a selection gone, can change which pane is on screen.
        self.read_on_screen_now(cx);
        // An agent finishing is usually what lets an update finish.
        self.finish_update_when_idle(cx);
        cx.notify();
    }

    pub fn visible_workspaces(&self) -> Vec<&Workspace> {
        self.workspaces.iter().filter(|w| crate::sidebar::lists(w)).collect()
    }

    pub fn select(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.close_home();
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
            let (busy, retitling) = (self.busy.clone(), self.retitling.contains(&id));
            pane.update(cx, |pane, cx| {
                pane.set_busy(busy, cx);
                pane.set_retitling(retitling, cx);
            });
            cx.subscribe(&pane, |t, _, event: &PaneEvent, cx| match event.clone() {
                PaneEvent::Error(message) => t.show_error(message, cx),
                PaneEvent::Archive(id, force) => t.archive(id, force, cx),
                PaneEvent::RegenerateTitle(id) => t.regenerate_title(id, cx),
            })
            .detach();
            self.panes.insert(id.clone(), pane);
        }
        // Only with a connection. Without one, `refresh` catches it up once there is one.
        if let Some(pane) = self.selected_pane() {
            pane.update(cx, WorkspacePane::shown);
        }
        self.focus_pane(id, window, cx);
        self.read_on_screen(window.is_window_active(), cx);
        cx.notify();
    }

    /// None without a connection, when panes are kept but neither shown nor used.
    fn selected_pane(&self) -> Option<Entity<WorkspacePane>> {
        if !matches!(self.connection, Connection::Ready(_)) {
            return None;
        }
        self.selected.as_ref().and_then(|id| self.panes.get(id).cloned())
    }
}

#[cfg(test)]
mod tests;
