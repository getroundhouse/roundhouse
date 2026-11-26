use super::failure::ConnectionFailure;
use super::selection::Selection;
use super::{home, hub_source, keeping_workers, menu_button, notifying, quitting, restarts, settings};
use crate::busy::Busy;
use crate::checkout::Checkout;
use crate::new_workspace::NewWorkspaceDialog;
use crate::tool_sign_in::ToolSigningIn;
use crate::workspace_pane::WorkspacePane;
use client_core::link::Link;
use client_core::records::Snapshot;
use client_core::{Client, Drafts, RecordsFeed};
use gpui_kit::{AnyWindowHandle, AppContext as _, Context, Entity, FocusHandle, ScrollHandle, Task, Window};
use hub_source::HubSource;
use protocol::client::HubStatus;
use protocol::model::{SessionId, Workspace, WorkspaceId};
use quitting::Quitting;
use restarts::UpdateQuestion;
use std::collections::{HashMap, HashSet};

/// How often the app looks whether an older hub has gone idle, which a test can wait out.
#[cfg(any(test, feature = "testing"))]
pub const LOOK_AGAIN: std::time::Duration = restarts::LOOK_AGAIN;

/// How long after a sound for a chat that needs the person the next one waits, which a test can wait out.
#[cfg(any(test, feature = "testing"))]
pub const SOUNDS_APART: std::time::Duration = notifying::SOUNDS_APART;

/// The position `by` away from `at` in a list of `len`, wrapping around. `at` has to be in the list,
/// and `by` no bigger than the list.
pub(crate) fn wrapped_step(at: usize, by: isize, len: usize) -> usize {
    (at + len).wrapping_add_signed(by) % len
}

pub enum Connection {
    Connecting,
    Ready(Client),
    Failed(ConnectionFailure),
    /// A hub is running, and there's a real protocol break between it and this app.
    AcrossABreak(HubStatus),
}

pub struct AppView {
    pub(crate) hub: HubSource,
    pub(super) visible: bool,
    pub(super) presentation: super::presentation::Pending,
    /// The checkout this build is from, which the sidebar names, with why it's open.
    pub(crate) checkout: Option<Checkout>,
    /// The connection to the hub, made again whenever it goes, and the hub's records, kept on every connection.
    pub(super) link: Link<HubSource>,
    /// What's shown of how the link stands. Set by [`Self::set_connection`], so the panes are told, and by
    /// `link_changed` once a new connection is ready.
    pub(super) connection: Connection,
    /// Whether the window was in front when the panes were last told whether they're drawn.
    pub(super) in_front: bool,
    /// Client-core's copy of the hub's records, which `records` shows.
    /// The hub's chats' conversations on this Mac, which every pane of its draws the bar of.
    pub(super) conversations: Entity<conversation::Conversations>,
    /// Keeps what's typed in each chat on the hub, for every pane, across connections.
    pub(super) drafts: Option<Drafts>,
    pub(super) restartable_hub: Option<restarts::OlderHub>,
    /// What's shown of the hub's records, as of client-core's last change to them.
    pub(super) records: Snapshot,
    /// The workspace selected, and those selected before it.
    pub(super) selection: Selection,
    /// Whether the selected workspace shows, or Home in its place.
    pub(super) main: home::Main,
    /// What Home shows, kept while it's put away.
    pub(crate) home: crate::home::Home,
    pub panes: HashMap<WorkspaceId, Entity<WorkspacePane>>,
    pub error: Option<String>,
    /// Where the sidebar's list is scrolled to. The list is made again each frame, so it can't keep that itself.
    pub(crate) sidebar_scroll: ScrollHandle,
    /// What the sidebar keeps from one frame to the next, besides where it's scrolled to.
    pub(crate) sidebar: crate::sidebar::Kept,
    /// The workspace whose row in the sidebar the pointer is over, which shows its archive button.
    pub(crate) pointing_at: Option<WorkspaceId>,
    pub busy: Busy,
    /// The workspaces the hub is finding a new title for, whose rows and headers say so.
    pub(crate) retitling: HashSet<WorkspaceId>,
    /// The new-workspace dialog, made the first time it opens, then kept with whatever was left in it.
    pub new_workspace: Option<Entity<NewWorkspaceDialog>>,
    /// The workspace selected while a dialog or Settings was open, whose pane is focused once that closes.
    pub(super) focus_once_closed: Option<WorkspaceId>,
    /// The mouse went down in the strip along the top, where no control took it, so moving it moves the window.
    pub(super) moving_window: bool,
    /// Whether a left press has had no release, for the release the system can keep.
    pub(super) unreleased: crate::lost_release::Unreleased,
    pub(super) focus: FocusHandle,
    /// Settings, when it shows in place of the sidebar and the workspace.
    pub(super) settings: Option<settings::Shown>,
    /// Where keys go while Settings shows.
    pub(super) settings_focus: FocusHandle,
    /// Signing in to one of the hub's MCP servers, which goes on whatever the window shows.
    pub(super) tool_sign_in: Entity<ToolSigningIn>,
    pub(super) window: AnyWindowHandle,
    /// Listening to the link, held so it stops when the view goes away.
    pub(super) following: Option<Task<()>>,
    /// What the update's card, or the dialog before restarting the hub now, asks before going ahead, while it asks.
    pub update_question: Option<UpdateQuestion>,
    /// The app's own request to update the hub it's connected to, if it made one.
    pub(super) hub_update_asked: Option<super::updating_hub::HubUpdateAsk>,
    pub quitting: Quitting,
    /// Reading what's running on the older hub, to ask about finishing the update or to go ahead with it, so pressing
    /// again doesn't read it twice.
    pub(super) asking: Option<Task<()>>,
    /// Looks every so often, while connected to an older hub, for whether it has gone idle, so the update can finish.
    pub(super) finishing: Option<Task<()>>,
    /// Asking the older hub whether it's idle.
    pub(super) looking: Option<Task<()>>,
    /// What the window shows of the hub beyond its records, once it has followed a hub's records.
    pub(super) shown: Option<super::pull_requests::Shown>,
    /// Draws the window again when what's said of a chat's cache next changes.
    pub(super) cache_clock: Option<Task<()>>,
    /// What the app goes on reading of the hub's status for the updater, as `keeping_workers` says.
    pub(super) keeping: keeping_workers::KeepingWorkers,
    /// The menus off the Mac, while they're open from the button in the corner.
    pub(super) menus_open: Option<menu_button::OpenMenus>,
    /// What the menus were last given to say.
    pub(super) menus_say: Option<crate::menus::Says>,
    /// Which chats have been told of as needing the person, and when that last sounded.
    pub(super) telling: notifying::Telling,
    /// The chat to show once its workspace's pane is made, as when a notification for a workspace with no pane yet is
    /// clicked, so the pane shows it from the start rather than reading another first.
    pub(super) chat_to_show: Option<(WorkspaceId, SessionId)>,
}

impl AppView {
    pub fn new(hub: HubSource, checkout: Option<Checkout>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        crate::updates::observe(cx).detach();
        // Settings says how dictation stands, and how far the model's download has got while it shows.
        crate::dictating::observe(cx, |_, cx| cx.notify()).detach();
        crate::dictating::observe_progress(cx, |t: &Self, _| t.settings_open()).detach();
        crate::dictating::keeping::observe(cx, Self::settings_open).detach();
        crate::settings::observe(cx, |_, cx| cx.notify()).detach();
        Self::observe_activation(window, cx);
        quitting::ask_first(window, cx);
        let tool_sign_in = cx.new(|_| ToolSigningIn::default());
        cx.observe(&tool_sign_in, |_, _, cx| cx.notify()).detach();
        let link = Link::start(hub.clone());
        let mut this = Self {
            hub,
            link,
            visible: true,
            presentation: Vec::new(),
            checkout,
            connection: Connection::Connecting,
            // Until the window says it's in front, so nothing is read that may not have been seen.
            in_front: false,
            conversations: crate::conversing::conversations(cx),
            drafts: None,
            restartable_hub: None,
            records: Snapshot::default(),
            selection: Selection::default(),
            main: home::Main::default(),
            home: crate::home::Home::new(window, cx),
            panes: HashMap::new(),
            error: None,
            sidebar_scroll: ScrollHandle::new(),
            sidebar: crate::sidebar::Kept::default(),
            pointing_at: None,
            busy: Busy::Free,
            retitling: HashSet::new(),
            new_workspace: None,
            focus_once_closed: None,
            moving_window: false,
            unreleased: crate::lost_release::Unreleased::default(),
            focus,
            settings: None,
            settings_focus: cx.focus_handle(),
            tool_sign_in,
            window: window.window_handle(),
            following: None,
            update_question: None,
            hub_update_asked: None,
            quitting: Quitting::No,
            asking: None,
            finishing: None,
            looking: None,
            shown: None,
            cache_clock: None,
            keeping: keeping_workers::KeepingWorkers::default(),
            menus_open: None,
            menus_say: None,
            telling: notifying::Telling::default(),
            chat_to_show: None,
        };
        this.follow_link(cx);
        this
    }

    fn observe_activation(window: &mut Window, cx: &mut Context<Self>) {
        // Coming back from another app is when someone reads the chat showing. It's also when Reduce Motion or a
        // permission may have changed in System Settings, which macOS doesn't tell the app about, so they're read again.
        // Permissions are read as the window opens too.
        crate::permissions::read(cx);
        cx.observe_window_activation(window, |t, window, cx| {
            gpui_kit::base::apply_system_reduce_motion(cx);
            if window.is_window_active() {
                crate::permissions::read(cx);
            }
            dictation_ui::window_activation_changed(window.window_handle(), window.is_window_active(), cx);
            t.read_on_screen(window.is_window_active(), cx);
        })
        .detach();
    }

    /// Whether the app is connected to a hub, or connecting, or why it isn't.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Has the app be connecting, or without a hub. Each pane holds what it would send until there's one again, and is
    /// told whether it's still drawn, which it is while connecting, with the window where it was last known to be,
    /// since this is often in the middle of updating it. A connection that's ready is told of by `refresh`, once the
    /// records have caught up with it.
    pub(super) fn set_connection(&mut self, connection: Connection, cx: &mut Context<Self>) {
        self.connection = connection;
        self.tell_tool_sign_in(cx);
        for pane in self.panes.values() {
            pane.update(cx, WorkspacePane::hub_went_away);
        }
        self.read_on_screen(self.in_front, cx);
    }

    /// Tells a sign-in to an MCP server which hub to ask from now on, if there's one.
    pub(super) fn tell_tool_sign_in(&self, cx: &mut Context<Self>) {
        let hub = self.client();
        self.tool_sign_in.update(cx, |signing_in, cx| signing_in.hub_is(hub, cx));
    }

    pub fn client(&self) -> Option<Client> {
        match &self.connection {
            Connection::Ready(client) => Some(client.clone()),
            _ => None,
        }
    }

    /// Client-core's copy of the hub's records, once it's following a hub.
    pub fn feed(&self) -> Option<RecordsFeed> {
        self.link.feed()
    }

    pub fn show_error(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.error = Some(message.into());
        cx.notify();
    }

    /// What's shown of client-core's copy of the hub's records, as of its last change.
    pub fn records(&self) -> &Snapshot {
        &self.records
    }

    pub fn visible_workspaces(&self) -> Vec<&Workspace> {
        self.records.workspaces().iter().filter(|w| crate::sidebar::lists(w)).collect()
    }

    /// Makes sure the selected workspace is on screen with its composer focused. This needs the window,
    /// and a click handler is already inside a window update, so it runs just after whatever called it.
    pub(super) fn show_selected(&mut self, cx: &mut Context<Self>) {
        let (handle, this) = (self.window, cx.weak_entity());
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |t, cx| t.make_selected_pane(window, cx));
            });
        });
    }

    /// None without a connection, when panes are kept but neither shown nor used.
    pub(super) fn selected_pane(&self) -> Option<Entity<WorkspacePane>> {
        if !matches!(self.connection, Connection::Ready(_)) {
            return None;
        }
        self.selected().and_then(|id| self.panes.get(id).cloned())
    }
}
