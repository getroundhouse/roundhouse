//! The app's one window: pairing with a hub, its workspaces, and a chat, one screen at a time.

mod hub_info;
mod new_workspace;
mod pairing;
mod workspaces;

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use client_core::{PairingLink, RemoteHub};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Theme};
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Task, Window, WindowOptions, div, px,
};
use protocol::model::RepoId;

use crate::chat::{ChatEvent, ChatView};
use crate::hub_link::{HubLink, Reconnected};
use crate::vault::Vault;

/// How long the app can be away before coming back connects again. A phone suspends an app soon after it
/// leaves the screen, and a connection it held may have gone without a word, but a glance at another app
/// shouldn't drop a working one.
const AWAY_BEFORE_RECONNECT: Duration = Duration::from_secs(20);

/// Opens the app's one window, after `gpui_kit::init`, on the hub `vault` keeps. On a phone the window fills the
/// screen whatever `options` asks. `links` carries pairing links the phone opens, such as from the Camera app.
pub fn open_window(
    vault: Arc<dyn Vault>,
    links: flume::Receiver<String>,
    options: WindowOptions,
    cx: &mut App,
) -> anyhow::Result<(AnyWindowHandle, Entity<MobileApp>)> {
    init(cx);
    gpui_kit::open_window(options, cx, move |window, cx| cx.new(|cx| MobileApp::new(vault, links, window, cx)))
}

/// The app's theme, with type sized for a phone held at arm's length rather than a desktop screen.
fn init(cx: &mut App) {
    look::apply(cx);
    let theme = Theme::global_mut(cx);
    theme.font_size = px(16.);
    theme.mono_font_size = px(13.);
}

/// The screen shown. Every one but pairing needs a hub, and shows pairing if there's none.
enum Screen {
    Pairing,
    Workspaces,
    NewWorkspace,
    HubInfo,
    /// The chat, and the subscription to what it asks of the app, which lasts as long as it's shown.
    Chat {
        chat: Entity<ChatView>,
        _events: Subscription,
    },
}

/// Where pairing has got to.
enum Pairing {
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
struct Followed {
    link: Entity<HubLink>,
    creating: Option<Creating>,
    _events: [Subscription; 2],
}

/// A workspace being made, in this repository.
struct Creating {
    repo: RepoId,
    _task: Task<()>,
}

pub struct MobileApp {
    vault: Arc<dyn Vault>,
    followed: Option<Followed>,
    screen: Screen,
    pairing: Pairing,
    link_input: Entity<InputState>,
    error: Option<SharedString>,
    /// By the wall clock, since an `Instant` on Apple's platforms stops while the phone sleeps.
    left_at: Option<SystemTime>,
    _activation: Subscription,
    _links: Task<()>,
}

impl MobileApp {
    fn new(
        vault: Arc<dyn Vault>,
        links: flume::Receiver<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> MobileApp {
        let link_input = cx.new(|cx| InputState::new(window, cx).placeholder("Pairing link"));
        let opened = cx.spawn(async move |this, cx| {
            while let Ok(url) = links.recv_async().await {
                if this.update(cx, |app, cx| app.pair(&url, cx)).is_err() {
                    return;
                }
            }
        });
        let activation = cx.observe_window_activation(window, Self::activation);
        let mut app = MobileApp {
            vault,
            followed: None,
            screen: Screen::Pairing,
            pairing: Pairing::Idle,
            link_input,
            error: None,
            left_at: None,
            _activation: activation,
            _links: opened,
        };
        if let Some(hub) = app.vault.load() {
            app.follow(hub, cx);
        }
        app
    }

    /// Starts following a hub, in place of any before it, and shows its workspaces.
    fn follow(&mut self, hub: RemoteHub, cx: &mut Context<Self>) {
        let link = cx.new(|cx| HubLink::new(hub, cx));
        let events = [
            cx.observe(&link, |_, _, cx| cx.notify()),
            cx.subscribe(&link, |app, _, Reconnected(client), cx| {
                if let Screen::Chat { chat, .. } = &app.screen {
                    chat.update(cx, |chat, cx| chat.reconnect(client, cx));
                }
            }),
        ];
        self.followed = Some(Followed { link, creating: None, _events: events });
        self.screen = Screen::Workspaces;
        cx.notify();
    }

    /// Stops following the hub and forgets it, so the phone can pair again.
    fn forget(&mut self, cx: &mut Context<Self>) {
        self.vault.forget();
        self.followed = None;
        self.pairing = Pairing::Idle;
        self.screen = Screen::Pairing;
        cx.notify();
    }

    fn activation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !window.is_window_active() {
            self.left_at = Some(SystemTime::now());
            return;
        }
        // A clock set back counts as away, since how long isn't known.
        let away =
            self.left_at.take().is_some_and(|left| !left.elapsed().is_ok_and(|away| away < AWAY_BEFORE_RECONNECT));
        if away && let Some(link) = self.link() {
            link.update(cx, HubLink::connect_now);
        }
    }

    fn link(&self) -> Option<&Entity<HubLink>> {
        self.followed.as_ref().map(|followed| &followed.link)
    }

    fn show_chat(&mut self, chat: Entity<ChatView>, cx: &mut Context<Self>) {
        let events = cx.subscribe(&chat, |app, _, ChatEvent::Back, cx| app.show(Screen::Workspaces, cx));
        self.show(Screen::Chat { chat, _events: events }, cx);
    }

    fn show(&mut self, screen: Screen, cx: &mut Context<Self>) {
        self.screen = screen;
        self.error = None;
        cx.notify();
    }

    /// Whether it's connected to the hub, and caught up.
    #[cfg(feature = "testing")]
    pub fn is_connected(&self, cx: &App) -> bool {
        self.link().is_some_and(|link| link.read(cx).client().is_some())
    }

    /// The chat being shown, once it has one open.
    #[cfg(feature = "testing")]
    pub fn transcript(&self, cx: &App) -> Option<Entity<transcript::Transcript>> {
        match &self.screen {
            Screen::Chat { chat, .. } => chat.read(cx).transcript(),
            Screen::Pairing | Screen::Workspaces | Screen::NewWorkspace | Screen::HubInfo => None,
        }
    }
}

impl Render for MobileApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let screen: AnyElement = match (&self.followed, &self.screen) {
            (None, _) | (Some(_), Screen::Pairing) => self.pairing_screen(window, cx),
            (Some(followed), Screen::Workspaces) => self.workspaces_screen(followed, window, cx),
            (Some(followed), Screen::NewWorkspace) => self.new_workspace_screen(followed, window, cx),
            (Some(followed), Screen::HubInfo) => Self::hub_info_screen(followed, window, cx),
            (Some(_), Screen::Chat { chat, .. }) => chat.clone().into_any_element(),
        };
        div().size_full().bg(cx.theme().background).text_color(cx.theme().foreground).child(screen)
    }
}
