//! The app's one window: pairing with a hub, its workspaces, and a chat, one screen at a time.

mod hub_info;
mod new_workspace;
mod pairing;
mod workspaces;

use std::sync::Arc;
use std::time::{Duration, Instant};

use client_core::{Client, ClientError};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Theme};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Task, Window, WindowOptions, div, px,
};
use protocol::model::RepoId;

use crate::chat::{ChatEvent, ChatView};
use crate::hub_link::{Connect, HubLink, Reconnected};
use crate::vault::Vault;
use client_core::{PairingLink, RemoteHub};

/// How long the app can be away before coming back connects again. A phone suspends an app soon after it
/// leaves the screen, and a connection it held may have gone without a word, but a glance at another app
/// shouldn't drop a working one.
const AWAY_BEFORE_RECONNECT: Duration = Duration::from_secs(20);

/// Pairs with the hub a link names, giving it the name to list this device under.
pub type Pair = Arc<dyn Fn(&PairingLink, &str) -> Result<RemoteHub, ClientError> + Send + Sync>;

/// Connects to a hub this phone paired with.
pub type ConnectTo = Arc<dyn Fn(&RemoteHub) -> Result<Client, ClientError> + Send + Sync>;

/// What the app needs from the phone, so tests can give it fakes.
pub struct Services {
    pub vault: Arc<dyn Vault>,
    pub pair: Pair,
    pub connect: ConnectTo,
}

impl Services {
    /// The phone's own: the vault it's given, and pairing and connecting over the network.
    pub fn device(vault: Arc<dyn Vault>, client_name: &'static str) -> Services {
        Services {
            vault,
            pair: Arc::new(client_core::pair),
            connect: Arc::new(move |hub| Client::connect_remote(hub, client_name)),
        }
    }
}

/// Opens the app's one window, after `gpui_kit::init`. On a phone it fills the screen whatever `options` asks.
pub fn open_window(
    services: Services,
    links: flume::Receiver<String>,
    options: WindowOptions,
    cx: &mut App,
) -> anyhow::Result<(AnyWindowHandle, Entity<MobileApp>)> {
    init(cx);
    gpui_kit::open_window(options, cx, move |window, cx| cx.new(|cx| MobileApp::new(services, links, window, cx)))
}

/// The app's theme, with type sized for a phone held at arm's length rather than a desktop screen.
fn init(cx: &mut App) {
    look::apply(cx);
    let theme = Theme::global_mut(cx);
    theme.font_size = px(16.);
    theme.mono_font_size = px(13.);
}

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
    Busy(SharedString),
    Failed(SharedString),
}

/// The hub being followed: the connection to it, what the vault keeps of it, and the subscriptions to the
/// connection, which last as long as it's followed.
struct Followed {
    link: Entity<HubLink>,
    hub: RemoteHub,
    _events: [Subscription; 2],
}

pub struct MobileApp {
    services: Services,
    followed: Option<Followed>,
    screen: Screen,
    pairing: Pairing,
    link_input: Entity<InputState>,
    /// The repository a new workspace is being made in.
    creating_in: Option<RepoId>,
    error: Option<SharedString>,
    left_at: Option<Instant>,
    _activation: Subscription,
    _links: Task<()>,
    working: Option<Task<()>>,
}

impl MobileApp {
    /// Opens on the hub the vault holds, or on pairing if it holds none. `links` carries pairing links the phone
    /// opens, such as from the Camera app's QR scanner.
    pub fn new(
        services: Services,
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
            services,
            followed: None,
            screen: Screen::Pairing,
            pairing: Pairing::Idle,
            link_input,
            creating_in: None,
            error: None,
            left_at: None,
            _activation: activation,
            _links: opened,
            working: None,
        };
        if let Some(hub) = app.services.vault.load() {
            app.follow(hub, cx);
        }
        app
    }

    /// Starts following a hub, and shows its workspaces.
    fn follow(&mut self, hub: RemoteHub, cx: &mut Context<Self>) {
        let connect_to = self.services.connect.clone();
        let target = hub.clone();
        let connect: Connect = Arc::new(move || connect_to(&target));
        let link = cx.new(|cx| HubLink::new(hub.hub_name.clone().into(), connect, cx));
        let events = [
            cx.observe(&link, |_, _, cx| cx.notify()),
            cx.subscribe(&link, |app, _, Reconnected(client), cx| {
                if let Screen::Chat { chat, .. } = &app.screen {
                    chat.update(cx, |chat, cx| chat.reconnect(client, cx));
                }
            }),
        ];
        self.followed = Some(Followed { link, hub, _events: events });
        self.screen = Screen::Workspaces;
        cx.notify();
    }

    /// Stops following the hub and forgets it, so the phone can pair again.
    fn forget(&mut self, cx: &mut Context<Self>) {
        self.services.vault.forget();
        self.followed = None;
        self.pairing = Pairing::Idle;
        self.screen = Screen::Pairing;
        cx.notify();
    }

    fn activation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !window.is_window_active() {
            self.left_at = Some(Instant::now());
            return;
        }
        let away = self.left_at.take().is_some_and(|left| left.elapsed() >= AWAY_BEFORE_RECONNECT);
        if away && let Some(link) = self.link() {
            link.update(cx, HubLink::connect_now);
        }
    }

    /// The screen shown, by name.
    #[cfg(feature = "testing")]
    pub fn showing(&self) -> &'static str {
        match &self.screen {
            Screen::Pairing => "pairing",
            Screen::Workspaces => "workspaces",
            Screen::NewWorkspace => "new workspace",
            Screen::HubInfo => "hub info",
            Screen::Chat { .. } => "chat",
        }
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
}

impl Render for MobileApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let screen = match &self.screen {
            Screen::Pairing => self.pairing_screen(window, cx).into_any_element(),
            Screen::Workspaces => self.workspaces_screen(window, cx).into_any_element(),
            Screen::NewWorkspace => self.new_workspace_screen(window, cx).into_any_element(),
            Screen::HubInfo => self.hub_info_screen(window, cx).into_any_element(),
            Screen::Chat { chat, .. } => chat.clone().into_any_element(),
        };
        div().size_full().bg(cx.theme().background).text_color(cx.theme().foreground).child(screen)
    }
}
