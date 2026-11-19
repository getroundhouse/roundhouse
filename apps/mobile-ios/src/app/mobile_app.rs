use std::sync::Arc;
use std::time::{Duration, SystemTime};

use client_core::RemoteHub;
use gpui_kit::component::input::InputState;
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Subscription, Task, Window, WindowOptions, div,
};
use protocol::model::{RepoId, SessionId};
use protocol::pairing::PairingLink;

use super::Panel;
use super::open::{OneAtATime, Open, OpenChat};
use crate::chat::{ChatEvent, ChatView};
use crate::hub_link::{HubLink, Reconnected};
use crate::vault::Vault;

/// How long the app can be away before coming back connects again. A phone suspends an app soon after it leaves the
/// screen, and a connection it held may have gone without a word, but a glance at another app shouldn't drop a working
/// one.
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
    pub(super) creating: Option<Creating>,
    _events: [Subscription; 2],
}

/// A workspace being made, in this repository.
pub(super) struct Creating {
    pub(super) repo: RepoId,
    pub(super) _task: Task<()>,
}

pub struct MobileApp {
    pub(super) vault: Arc<dyn Vault>,
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
    fn new(vault: Arc<dyn Vault>, links: flume::Receiver<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
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
            open: Open::default(),
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

    /// Starts following a hub, in place of any before it, with nothing of the last one open.
    pub(super) fn follow(&mut self, hub: RemoteHub, cx: &mut Context<Self>) {
        let link = cx.new(|cx| HubLink::new(hub, cx));
        let events = [
            cx.observe(&link, |app, link, cx| {
                let records = link.read(cx).snapshot();
                if let (Some(open), Some(records)) = (&app.open.chat, records) {
                    open.chat.update(cx, |chat, cx| chat.records_changed(&records, cx));
                }
                cx.notify();
            }),
            cx.subscribe(&link, |app, _, Reconnected(client), cx| {
                if let Some(open) = &app.open.chat {
                    open.chat.update(cx, |chat, cx| chat.reconnect(client, cx));
                }
            }),
        ];
        self.followed = Some(Followed { link, creating: None, _events: events });
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
        self.error = None;
        self.followed = None;
        self.open = Open::default();
        self.pairing = Pairing::Idle;
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

    pub(super) fn link(&self) -> Option<&Entity<HubLink>> {
        self.followed.as_ref().map(|followed| &followed.link)
    }

    /// Opens `chat`, in place of any panel.
    pub(super) fn open_chat(&mut self, chat: Entity<ChatView>, window: &mut Window, cx: &mut Context<Self>) {
        let events = cx.subscribe_in(&chat, window, |app, _, event: &ChatEvent, window, cx| match event {
            ChatEvent::Back => {
                app.close_chat(None, cx);
                cx.notify();
            }
            ChatEvent::GoToWorkspace(workspace) => app.open_workspace(workspace, window, cx),
        });
        let opening = chat.read(cx).session().cloned();
        self.close_chat(opening.as_ref(), cx);
        self.open = Open { chat: Some(OpenChat { chat, _events: events }), panel: None };
        self.error = None;
        cx.notify();
    }

    /// Closes the open chat, and lets go of its records, unless `opening`, the chat that opens in its place, shows the
    /// same one. Following a chat's records isn't counted, so letting go after the new chat follows them would stop
    /// both.
    fn close_chat(&mut self, opening: Option<&SessionId>, cx: &mut Context<Self>) {
        let Some(open) = self.open.chat.take() else { return };
        let closing = open.chat.read(cx);
        if closing.session() != opening {
            closing.let_go();
        }
    }

    /// Opens a panel over the list, or closes the one that's open.
    pub(super) fn show_panel(&mut self, panel: Option<Panel>, cx: &mut Context<Self>) {
        self.open.panel = panel;
        self.error = None;
        cx.notify();
    }

    /// Whether it's connected to the hub, and caught up.
    #[cfg(feature = "testing")]
    pub fn is_connected(&self, cx: &App) -> bool {
        self.link().is_some_and(|link| link.read(cx).client().is_some())
    }

    /// What the open chat's header names it.
    #[cfg(feature = "testing")]
    pub fn chat_title(&self, cx: &App) -> Option<String> {
        self.open.chat.as_ref().map(|open| open.chat.read(cx).title().to_string())
    }

    /// The chat that's open, once it has one showing.
    #[cfg(feature = "testing")]
    pub fn transcript(&self, cx: &App) -> Option<Entity<transcript::Transcript>> {
        self.open.chat.as_ref().and_then(|open| open.chat.read(cx).transcript())
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
        div().size_full().bg(pane).text_color(text).child(screen)
    }
}
