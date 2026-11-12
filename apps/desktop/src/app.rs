//! The window: the sidebar on the left and the selected workspace beside it. Shows the hub's records,
//! which client-core keeps current, and is the only place that connects and reconnects.

use std::collections::{HashMap, HashSet};

use client_core::{Client, Drafts, HubAdmin, RecordsFeed};
use gpui_kit::component::VirtualListScrollHandle;
use gpui_kit::{AnyWindowHandle, AppContext, Context, Entity, FocusHandle, Task, Window};
use protocol::client::HubStatus;
use protocol::model::{Repo, Session, Workspace, WorkspaceId};

use crate::busy::Busy;
use crate::checkout::Checkout;
use crate::new_workspace::NewWorkspaceDialog;
use crate::workspace_pane::WorkspacePane;

mod build_line;
mod corner;
mod home;
mod hub_source;
mod pull_requests;
mod quitting;
mod reading;
mod records;
mod restarts;
mod settings;
mod shortcuts;
mod title_bar;
mod view;
mod workspaces;

pub use build_line::{Agents, BuildLine};
pub use hub_source::HubSource;
pub use quitting::Quitting;
pub(crate) use quitting::quit;
pub use restarts::UpdateQuestion;
pub use title_bar::TitleBar;
#[cfg(feature = "testing")]
pub use title_bar::the_title_bar_is;

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
    /// Keeps what's typed in each chat on the hub, for every pane, across connections.
    drafts: Option<Drafts>,
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
    /// What the sidebar keeps from one frame to the next, besides where it's scrolled to.
    pub(crate) sidebar: crate::sidebar::Kept,
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
    /// Whether a left press has had no release, for the release the system can keep.
    unreleased: crate::lost_release::Unreleased,
    focus: FocusHandle,
    /// Settings, when it shows in place of the sidebar and the workspace.
    settings: Option<settings::Shown>,
    /// Where keys go while Settings shows.
    settings_focus: FocusHandle,
    window: AnyWindowHandle,
    /// Held so these tasks stop when the view goes away.
    connecting: Option<Task<()>>,
    following: Option<Task<()>>,
    /// What the update's card, or the dialog before restarting the hub now, asks before going ahead, while it asks.
    pub update_question: Option<UpdateQuestion>,
    pub quitting: Quitting,
    /// Reading what's running on the older hub, to ask about finishing the update or to go ahead with it, so pressing
    /// again doesn't read it twice.
    asking: Option<Task<()>>,
    /// Looks every so often, while connected to an older hub, for whether it has gone idle, so the update can finish.
    finishing: Option<Task<()>>,
    /// Asking the older hub whether it's idle.
    looking: Option<Task<()>>,
    _watching_pull_requests: Task<()>,
    /// Draws the window again when what's said of a chat's cache next changes.
    cache_clock: Option<Task<()>>,
    /// What the menu bar was last given to say.
    menus_say: crate::menus::Says,
}

impl AppView {
    pub fn new(hub: HubSource, checkout: Option<Checkout>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        crate::updates::observe(cx).detach();
        crate::settings::observe(cx, |_, cx| cx.notify()).detach();
        // Coming back from another app is when someone reads the chat showing. It's also when Reduce Motion may have
        // changed in System Settings, which macOS doesn't tell the kit about, so it's read again.
        cx.observe_window_activation(window, |t, window, cx| {
            gpui_kit::base::apply_system_reduce_motion(cx);
            t.read_on_screen(window.is_window_active(), cx);
        })
        .detach();
        quitting::ask_first(window, cx);
        let mut this = Self {
            hub,
            checkout,
            connection: Connection::Connecting,
            feed: None,
            drafts: None,
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
            sidebar: crate::sidebar::Kept::default(),
            pointing_at: None,
            busy: Busy::Free,
            retitling: HashSet::new(),
            new_workspace: None,
            focus_after_dialog: None,
            moving_window: false,
            unreleased: crate::lost_release::Unreleased::default(),
            focus,
            settings: None,
            settings_focus: cx.focus_handle(),
            window: window.window_handle(),
            connecting: None,
            following: None,
            update_question: None,
            quitting: Quitting::No,
            asking: None,
            finishing: None,
            looking: None,
            _watching_pull_requests: Self::watch_pull_requests(cx),
            cache_clock: None,
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

    /// Client-core's copy of the hub's records, once it's following a hub.
    pub fn feed(&self) -> Option<RecordsFeed> {
        self.feed.clone()
    }

    pub fn show_error(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.error = Some(message.into());
        cx.notify();
    }

    /// Connects. Everything already shown carries on on the new connection, and the first time, client-core
    /// starts following the hub's records. Comes back here if the connection goes, which starts a new hub.
    pub fn connect(&mut self, cx: &mut Context<Self>) {
        self.show_connecting(cx);
        // What it was connected to says nothing about what it connects to next.
        (self.restartable_hub, self.finishing, self.looking) = (None, None, None);
        self.drop_restart_question(cx);
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
        self.keep_drafts_on(&client, cx);
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
                self.drop_restart_question(cx);
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

    pub fn visible_workspaces(&self) -> Vec<&Workspace> {
        self.workspaces.iter().filter(|w| crate::sidebar::lists(w)).collect()
    }

    pub fn select(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        self.close_home();
        self.show_selected(cx);
        cx.notify();
    }

    /// Moves `by` workspaces down the sidebar, in the order it lists them.
    fn step_selection(&mut self, by: isize, cx: &mut Context<Self>) {
        let visible = crate::sidebar::listed(self, cx);
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
