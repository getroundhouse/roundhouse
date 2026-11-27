use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use client_core::RemoteHub;
use client_core::this_device::Preferences;
use gpui_kit::component::input::InputState;
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Subscription, Task, Window, WindowOptions, div,
};
use protocol::model::{RepoId, WorkspaceId};
use protocol::pairing::PairingLink;

use super::Panel;
use super::open::{OneAtATime, Open, OpenChat};
use super::workspace_menu::{Asking, Renaming};
use crate::chat::{ChatEvent, ChatView};
use crate::hub_link::{self, ConnectTo, HubLink};
use crate::vault::Vault;

/// How long the app can be away before coming back connects again. A phone suspends an app soon after it leaves the
/// screen, and a connection it held may have gone without a word, but a glance at another app shouldn't drop a working
/// one.
const AWAY_BEFORE_RECONNECT: Duration = Duration::from_secs(20);

/// Opens the app's one window, after `gpui_kit::init`, on the hub `vault` keeps, where `preferences` says the app was
/// left. On a phone the window fills the screen whatever `options` asks. `links` carries pairing links the phone opens,
/// such as from the Camera app.
pub fn open_window(
    vault: Arc<dyn Vault>,
    preferences: Preferences,
    links: flume::Receiver<String>,
    options: WindowOptions,
    cx: &mut App,
) -> anyhow::Result<(AnyWindowHandle, Entity<MobileApp>)> {
    open(vault, preferences, links, hub_link::over_the_network(), options, cx)
}

/// As [`open_window`], on an app never left anywhere, connecting to the hub it's paired with as `connect` does, such as
/// to a test's own hub on a socket, rather than over the network.
#[cfg(feature = "testing")]
pub fn open_window_connecting(
    vault: Arc<dyn Vault>,
    links: flume::Receiver<String>,
    connect: impl Fn(&RemoteHub) -> Result<client_core::Connection, client_core::ClientError> + Send + Sync + 'static,
    options: WindowOptions,
    cx: &mut App,
) -> anyhow::Result<(AnyWindowHandle, Entity<MobileApp>)> {
    open(vault, Preferences::in_memory(), links, Arc::new(connect), options, cx)
}

fn open(
    vault: Arc<dyn Vault>,
    preferences: Preferences,
    links: flume::Receiver<String>,
    connect: ConnectTo,
    options: WindowOptions,
    cx: &mut App,
) -> anyhow::Result<(AnyWindowHandle, Entity<MobileApp>)> {
    init(cx);
    gpui_kit::open_window(options, cx, move |window, cx| {
        cx.new(|cx| MobileApp::new(vault, preferences, links, connect, window, cx))
    })
}

/// The app's look, for a device held in the hand and pressed by touch, as "Each client" in `docs/spec/look.md` says.
fn init(cx: &mut App) {
    look::Reach::Handheld.choose();
    look::apply(cx);
    look::Input::Touch.choose(cx);
}

/// Where pairing has got to.
pub(super) enum Pairing {
    Idle,
    /// A link to another hub, waiting for a yes, since pairing with it replaces the hub this phone has. Any app can
    /// open a link, so one arriving isn't enough.
    Confirm(PairingLink),
    Busy {
        hub: SharedString,
        _task: Task<()>,
    },
    Failed(SharedString),
}

/// The hub being followed: the connection to it, the workspace being made in it, and the subscriptions to the
/// connection. All of it goes with the hub.
pub(super) struct Followed {
    pub(super) link: Entity<HubLink>,
    /// The app is still to open where it was left, once it has the hub's records.
    pub(super) reopens: bool,
    pub(super) creating: Option<Creating>,
    /// What's asked before it's done to one of its workspaces, over the screen.
    pub(super) asking: Option<Asking>,
    /// One of its workspaces being renamed, in a sheet over the screen.
    pub(super) renaming: Option<Renaming>,
    /// Its workspaces whose new title is on its way, which aren't asked for another meanwhile.
    pub(super) retitling: HashSet<WorkspaceId>,
    /// Its workspaces asked to be archived, whose Archive is off until the hub has answered.
    pub(super) archiving: HashSet<WorkspaceId>,
    _events: Subscription,
}

/// A workspace being made, in this repository.
pub(super) struct Creating {
    pub(super) repo: RepoId,
    pub(super) _task: Task<()>,
}

pub struct MobileApp {
    pub(super) vault: Arc<dyn Vault>,
    /// What the phone remembers from how it's used, such as the workspace whose chat is open, to open on again.
    pub(super) preferences: Preferences,
    connect: ConnectTo,
    pub(super) followed: Option<Followed>,
    pub(super) open: Open,
    pub(super) pairing: Pairing,
    pub(super) link_input: Entity<InputState>,
    pub(super) error: Option<SharedString>,
    /// By the wall clock, since an `Instant` on Apple's platforms stops while the phone sleeps.
    left_at: Option<SystemTime>,
    _activation: Subscription,
    _links: Task<()>,
}

impl MobileApp {
    fn new(
        vault: Arc<dyn Vault>,
        preferences: Preferences,
        links: flume::Receiver<String>,
        connect: ConnectTo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let link_input = cx.new(|cx| InputState::new(window, cx).placeholder("Pairing link"));
        let opened = cx.spawn_in(window, async move |this, cx| {
            while let Ok(url) = links.recv_async().await {
                if this.update_in(cx, |app, window, cx| app.pair(&url, window, cx)).is_err() {
                    return;
                }
            }
        });
        let activation = cx.observe_window_activation(window, Self::activation);
        let mut app = MobileApp {
            vault,
            preferences,
            connect,
            followed: None,
            open: Open::default(),
            pairing: Pairing::Idle,
            link_input,
            error: None,
            left_at: None,
            _activation: activation,
            _links: opened,
        };
        if let Some(hub) = app.vault.load() {
            app.follow(hub, window, cx);
        }
        app
    }

    /// Starts following a hub, in place of any before it, with nothing of the last one open, nor kept of where the app
    /// was left on it.
    pub(super) fn follow(&mut self, hub: RemoteHub, window: &mut Window, cx: &mut Context<Self>) {
        if self.link().is_some_and(|link| link.read(cx).paired().fingerprint != hub.fingerprint) {
            self.left_on(None, cx);
        }
        let connect = self.connect.clone();
        let link = cx.new(|cx| HubLink::new(hub, connect, cx));
        let events = cx.observe_in(&link, window, |app, link, window, cx| {
            let (records, hub_here) = (link.read(cx).snapshot(), link.read(cx).is_ready());
            if let (Some(open), Some(records)) = (&app.open.chat, records) {
                open.chat.update(cx, |chat, cx| {
                    chat.set_hub_here(hub_here, cx);
                    chat.records_changed(&records, cx);
                });
            }
            app.reopen(window, cx);
            cx.notify();
        });
        let followed = Followed {
            link,
            reopens: true,
            creating: None,
            asking: None,
            renaming: None,
            retitling: HashSet::new(),
            archiving: HashSet::new(),
            _events: events,
        };
        self.followed = Some(followed);
        self.open = Open::default();
        cx.notify();
    }

    /// Stops following the hub and forgets it, so the phone can pair again. One the vault couldn't forget would be
    /// followed again the next time the app starts, so the phone stays with it, and says why.
    pub(super) fn forget(&mut self, cx: &mut Context<Self>) {
        if let Err(e) = self.vault.forget() {
            self.error = Some(format!("Couldn't forget the hub: {e}").into());
            cx.notify();
            return;
        }
        self.left_on(None, cx);
        self.error = None;
        self.followed = None;
        self.open = Open::default();
        self.pairing = Pairing::Idle;
        cx.notify();
    }

    fn activation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(open) = &self.open.chat {
            let in_front = window.is_window_active();
            open.chat.update(cx, |chat, cx| chat.set_app_in_front(in_front, cx));
        }
        if !window.is_window_active() {
            self.left_at = Some(SystemTime::now());
            if let Some(link) = self.link() {
                link.read(cx).keep_drafts_now();
            }
            return;
        }
        // A clock set back counts as away, since how long isn't known.
        let away =
            self.left_at.take().is_some_and(|left| !left.elapsed().is_ok_and(|away| away < AWAY_BEFORE_RECONNECT));
        if away && let Some(link) = self.link() {
            link.update(cx, HubLink::connect_now);
        }
    }

    pub(super) fn link(&self) -> Option<&Entity<HubLink>> {
        self.followed.as_ref().map(|followed| &followed.link)
    }

    /// The hub this phone paired with, while it follows one.
    pub(super) fn hub(&self, cx: &App) -> Option<client_core::Hub> {
        self.link().map(|link| link.read(cx).hub().clone())
    }

    /// The hub this phone paired with, while it has a connection to it that has caught up, which is when the phone
    /// asks it anything.
    pub(super) fn ready_hub(&self, cx: &App) -> Option<client_core::Hub> {
        self.link().filter(|link| link.read(cx).is_ready()).map(|link| link.read(cx).hub().clone())
    }

    /// Says what went wrong, across the screen under its bar.
    pub(super) fn say(&mut self, error: String, cx: &mut Context<Self>) {
        self.error = Some(error.into());
        cx.notify();
    }

    /// Opens `chat`, in place of any panel.
    pub(super) fn open_chat(&mut self, chat: Entity<ChatView>, window: &mut Window, cx: &mut Context<Self>) {
        let events = cx.subscribe_in(&chat, window, |app, _, event: &ChatEvent, window, cx| match event {
            ChatEvent::Back => {
                app.open.chat = None;
                app.left_on(None, cx);
                cx.notify();
            }
            ChatEvent::GoToWorkspace(workspace) => app.open_workspace(workspace, window, cx),
        });
        self.left_on(Some(chat.read(cx).workspace()), cx);
        // The chat that was open goes, and lets go of what it followed of its session as it does.
        self.open = Open { chat: Some(OpenChat { chat, _events: events }), panel: None };
        self.error = None;
        cx.notify();
    }

    /// Opens a panel over the list, or closes the one that's open.
    pub(super) fn show_panel(&mut self, panel: Option<Panel>, cx: &mut Context<Self>) {
        self.open.panel = panel;
        self.error = None;
        cx.notify();
    }

    /// Whether it's connected to the hub, and caught up.
    #[cfg(any(test, feature = "testing"))]
    pub fn is_connected(&self, cx: &App) -> bool {
        self.link().is_some_and(|link| link.read(cx).is_ready())
    }

    /// How the link to the hub stands, with why its last try to connect failed, if it did, for a test that timed out
    /// waiting on it to say.
    #[cfg(any(test, feature = "testing"))]
    pub fn link_state(&self, cx: &App) -> String {
        match self.link() {
            Some(link) => format!("{:?}", link.read(cx).state()),
            None => "none, since the phone follows no hub".to_string(),
        }
    }

    /// Connects to the hub again now, as coming back to the app after a while away does.
    #[cfg(feature = "testing")]
    pub fn connect_again(&mut self, cx: &mut Context<Self>) {
        if let Some(link) = self.link() {
            link.update(cx, HubLink::connect_now);
        }
    }

    /// What the open chat's header names it.
    #[cfg(any(test, feature = "testing"))]
    pub fn chat_title(&self, cx: &App) -> Option<String> {
        self.open.chat.as_ref().map(|open| open.chat.read(cx).title().to_string())
    }

    /// The chat that's open, once it has one showing.
    #[cfg(any(test, feature = "testing"))]
    pub fn transcript(&self, cx: &App) -> Option<Entity<transcript::Transcript>> {
        self.open.chat.as_ref().and_then(|open| open.chat.read(cx).transcript())
    }

    /// The chat the open chat's screen shows, once it shows one.
    #[cfg(feature = "testing")]
    pub fn chat_shown(&self, cx: &App) -> Option<protocol::model::SessionId> {
        self.open.chat.as_ref().and_then(|open| open.chat.read(cx).session().cloned())
    }

    /// The phone's own record of the chat the open chat's screen shows, which can be behind the hub's.
    #[cfg(feature = "testing")]
    pub fn shown_record(&self, cx: &App) -> Option<protocol::model::Session> {
        self.open.chat.as_ref().and_then(|open| open.chat.read(cx).shown_record().cloned())
    }

    /// What the open chat says went wrong, while it says it.
    #[cfg(feature = "testing")]
    pub fn chat_says(&self, cx: &App) -> Option<String> {
        self.open.chat.as_ref().and_then(|open| open.chat.read(cx).said())
    }

    /// What's typed in the open chat's composer.
    #[cfg(feature = "testing")]
    pub fn typed(&self, cx: &App) -> Option<String> {
        self.open.chat.as_ref().map(|open| open.chat.read(cx).typed(cx))
    }

    /// The work of the subagent shown over the open chat, while it's in front.
    #[cfg(feature = "testing")]
    pub fn subagent_transcript(&self, cx: &App) -> Option<Entity<transcript::Transcript>> {
        self.open.chat.as_ref().and_then(|open| open.chat.read(cx).subagent_transcript())
    }
}

impl Render for MobileApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let (pane, text) = (colors.pane, colors.text);
        // A link opened while paired is answered on the pairing screen, over whatever's open.
        let pairing = !matches!(self.pairing, Pairing::Idle);
        let screen: AnyElement = match &self.followed {
            Some(followed) if !pairing => match self.open.one_at_a_time() {
                OneAtATime::List => self.workspaces_screen(followed, window, cx),
                OneAtATime::Panel(Panel::Hub) => self.hub_screen(followed, window, cx),
                OneAtATime::Panel(Panel::NewWorkspace) => self.new_workspace_screen(followed, window, cx),
                OneAtATime::Chat(chat) => chat.clone().into_any_element(),
            },
            Some(_) | None => self.pairing_screen(window, cx),
        };
        let over = [self.asking_sheet(window, cx), self.renaming_sheet(window, cx)];
        div().relative().size_full().bg(pane).text_color(text).child(screen).children(over.into_iter().flatten())
    }
}
