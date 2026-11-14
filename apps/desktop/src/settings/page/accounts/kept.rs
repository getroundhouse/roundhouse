//! What Accounts keeps from one frame to the next while it shows: the panel for connecting a login, and what the hub
//! last answered. A token pasted into the panel is handed to the hub and kept nowhere else: not on disk, not in a log
//! and not in the app's settings. Its field is made anew once the hub has answered, so nothing of it stays.

use std::time::Duration;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::{
    App, AppContext as _, Bounds, Context, Entity, FocusHandle, Pixels, PromptLevel, ScrollHandle, Subscription, Task,
    Window, point, px,
};
use protocol::model::{Account, AccountId, AgentKind, LoginToken, Record};

use super::Asking;
use super::words::{self, Refused};

/// The page's state, kept only while it shows, so a token half pasted goes with it, and so does the wait for what the
/// hub answers to one.
#[derive(Default)]
pub(super) struct Kept {
    pub(super) panel: Option<Panel>,
    /// The hub refused to connect a login as too old to keep any.
    pub(super) refused_as_older: bool,
    /// Why the hub couldn't forget the login of the account under whose row it's said.
    not_disconnected: Option<(AccountId, String)>,
    /// The hub to ask, as of the page's last frame, with client-core's copy of its records, which takes in the record
    /// an answer carries. `None` while there's no hub to ask.
    pub(super) asking: Option<Asking>,
    pub(super) choosing: super::choosing::Choosing,
    /// Settings' focus, as of the page's last frame, where focus goes back to as the panel goes while it has it, so
    /// keys still reach Settings.
    settings_focus: Option<FocusHandle>,
    /// Draws the page again when what it says of a time next changes, and when that is.
    clock: Option<(i64, Task<()>)>,
}

/// The panel for connecting a login of `agent`'s kind: as a new account, or in place of an account's own.
pub(super) struct Panel {
    pub(super) agent: AgentKind,
    pub(super) to: To,
    /// Where the token is pasted, shown masked.
    pub(super) token: Field,
    /// What the hub said when it last refused, for the person to read.
    pub(super) refused: Option<String>,
    /// The wait for what the hub answers, while it hasn't. It goes with the panel, so an answer to a panel that was
    /// closed, or replaced by another, lands on none.
    connecting: Option<Task<()>>,
    /// Whether to bring the panel into sight as it's next laid out, which it is once as it opens.
    reveal: bool,
    /// Holds the focus of everything in the panel, to tell whether it has it as it goes.
    pub(super) focus: FocusHandle,
}

impl Panel {
    /// Whether it's the panel for the login of the account `id`.
    pub(super) fn is_for(&self, id: &AccountId) -> bool {
        matches!(&self.to, To::Account { id: of, .. } if of == id)
    }
}

/// A field of the panel's, which connects as Enter is pressed in it.
pub(super) struct Field {
    pub(super) input: Entity<InputState>,
    _entered: Subscription,
}

impl Field {
    /// A field made with `made`, with the cursor in it.
    fn new(window: &mut Window, cx: &mut Context<Kept>, made: impl FnOnce(InputState) -> InputState) -> Field {
        let input = cx.new(|cx| made(InputState::new(window, cx)));
        let entered = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.connect(window, cx);
            }
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        Field { input, _entered: entered }
    }

    /// A field for a token, whose text shows only as dots, and can't be copied from.
    fn token(window: &mut Window, cx: &mut Context<Kept>) -> Field {
        Field::new(window, cx, |input| input.masked(true))
    }

    /// Whether anything but spaces is in it.
    pub(super) fn filled(&self, cx: &App) -> bool {
        !self.input.read(cx).value().trim().is_empty()
    }

    fn value(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }
}

/// What a panel connects a login to.
pub(super) enum To {
    /// A new account, called what's typed here.
    New { name: Field },
    /// The account `id`, called `name`, in place of the login the hub `held` for it, as when that's about to end, or
    /// as its login again once it was disconnected.
    Account { id: AccountId, name: String, held: bool },
}

impl Kept {
    /// What the page was drawn with: the hub to ask, Settings' focus, and when what it says of a time next changes, if
    /// it says one.
    pub(super) fn drawn(
        &mut self,
        asking: Option<Asking>,
        settings_focus: FocusHandle,
        changes_at_ms: Option<i64>,
        cx: &mut Context<Self>,
    ) {
        (self.asking, self.settings_focus) = (asking, Some(settings_focus));
        if self.clock.as_ref().map(|(at_ms, _)| *at_ms) == changes_at_ms {
            return;
        }
        self.clock = changes_at_ms.map(|at_ms| {
            let wait = Duration::from_millis(at_ms.saturating_sub(look::now_ms(cx)).max(0).unsigned_abs());
            let task = cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, |this, cx| {
                    this.clock = None;
                    cx.notify();
                });
            });
            (at_ms, task)
        });
    }

    /// Whether there's a hub to ask.
    pub(super) fn can_ask(&self) -> bool {
        self.asking.is_some()
    }

    /// Why the hub couldn't forget the login of the account `id`, to say under its row, if it couldn't.
    pub(super) fn not_disconnected(&self, id: &AccountId) -> Option<&str> {
        self.not_disconnected.as_ref().filter(|(of, _)| of == id).map(|(_, said)| said.as_str())
    }

    /// Opens the panel for connecting a login of `agent`'s kind as a new account, with a field for its name.
    pub(super) fn open_new(&mut self, agent: AgentKind, window: &mut Window, cx: &mut Context<Self>) {
        let name = Field::new(window, cx, |input| input);
        self.open(agent, To::New { name }, window, cx);
    }

    /// Opens the panel for connecting a login in place of `account`'s own, as renewing it does, or connecting it again.
    pub(super) fn open_for(
        &mut self,
        agent: AgentKind,
        account: &Account,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to =
            To::Account { id: account.id.clone(), name: words::called(account), held: account.held_login.is_some() };
        self.open(agent, to, window, cx);
    }

    /// Opens the panel, in place of any that was open, with the cursor where the token is pasted.
    fn open(&mut self, agent: AgentKind, to: To, window: &mut Window, cx: &mut Context<Self>) {
        let token = Field::token(window, cx);
        let focus = cx.focus_handle();
        self.panel = Some(Panel { agent, to, token, refused: None, connecting: None, reveal: true, focus });
        self.not_disconnected = None;
        cx.notify();
    }

    pub(super) fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.put_away_panel(window, cx);
    }

    /// Takes the panel away, and the wait for the hub's answer with it. Its fields go too, so focus one of them had,
    /// or anything else in it, goes back to Settings, where keys still reach it.
    fn put_away_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.panel.take() else { return };
        if let Some(settings) = &self.settings_focus
            && panel.focus.contains_focused(window, cx)
        {
            window.focus(settings, cx);
        }
        cx.notify();
    }

    /// Whether the panel has what connecting takes, and isn't waiting on the hub, with a hub to ask.
    pub(super) fn can_connect(&self, cx: &App) -> bool {
        let Some(panel) = &self.panel else { return false };
        let named = match &panel.to {
            To::New { name } => name.filled(cx),
            To::Account { .. } => true,
        };
        self.can_ask() && panel.connecting.is_none() && panel.token.filled(cx) && named
    }

    /// Hands the hub the token pasted, and the name, for the account the panel is for.
    pub(super) fn connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_connect(cx) {
            return;
        }
        let (Some(asking), Some(panel)) = (self.asking.clone(), self.panel.as_mut()) else { return };
        let token = LoginToken::new(panel.token.value(cx));
        let (name, account) = match &panel.to {
            To::New { name } => (name.value(cx), None),
            To::Account { id, name, .. } => (name.clone(), Some(id.clone())),
        };
        let (agent, renewing) = (panel.agent, account.is_some());
        let connecting = asking.client.start(move |client| client.account_connect(agent, &name, token, account));
        panel.connecting = Some(cx.spawn_in(window, async move |this, cx| {
            let answer = connecting.recv().await;
            let _ = this.update_in(cx, |this, window, cx| this.answered(answer, renewing, &asking, window, cx));
        }));
        cx.notify();
    }

    /// Closes the panel once the hub has connected the login, as a new account or, when `renewing`, in place of an
    /// account's, and otherwise says why it couldn't, with the token's field made anew, so what was pasted is gone. The
    /// panel is the one that asked, since the wait goes with it.
    fn answered(
        &mut self,
        answer: client_core::Result<Account>,
        renewing: bool,
        asked: &Asking,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match answer.map_err(|e| words::refused(&e, renewing)) {
            Ok(account) => {
                self.put_away_panel(window, cx);
                asked.feed.take_reply(Record::Account(account));
            }
            Err(Refused::OlderHub) => {
                self.put_away_panel(window, cx);
                self.refused_as_older = true;
            }
            Err(Refused::Said(said)) => {
                let token = Field::token(window, cx);
                let Some(panel) = self.panel.as_mut() else { return };
                (panel.token, panel.refused, panel.connecting) = (token, Some(said), None);
                cx.notify();
            }
        }
    }

    /// Brings the panel into sight on Settings' `scroll`, once as it opens, now that it's laid out at `bounds`. This is
    /// during a draw, after the page has been placed by the scroll as it was, so the scroll shows in the frame after,
    /// which is asked for once this one is done, since the window can't be marked for drawing again while it draws.
    pub(super) fn laid_out(&mut self, bounds: Bounds<Pixels>, scroll: &ScrollHandle, window: &mut Window) {
        let Some(panel) = self.panel.as_mut().filter(|panel| panel.reveal) else { return };
        panel.reveal = false;
        if bring_into_sight(scroll, bounds) {
            window.on_next_frame(|window, _| window.refresh());
        }
    }

    /// Asks whether to have the hub forget the login of the account `id`, called `name`, and does once it's confirmed.
    /// What was said of an earlier try goes, since this is another.
    pub(super) fn ask_to_disconnect(&mut self, id: AccountId, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.not_disconnected = None;
        let asked = words::disconnect_asks(name);
        let answer =
            window.prompt(PromptLevel::Warning, &asked, Some(words::DISCONNECT_SAYS), &["Cancel", "Disconnect"], cx);
        cx.spawn(async move |this, cx| {
            if answer.await == Ok(1) {
                let _ = this.update(cx, |this, cx| this.disconnect(id, cx));
            }
        })
        .detach();
    }

    /// Has the hub forget the login of the account `id`, and says under its row why it couldn't, if it couldn't, as
    /// with no hub to ask.
    pub(super) fn disconnect(&mut self, id: AccountId, cx: &mut Context<Self>) {
        let Some(asking) = self.asking.clone() else {
            self.not_disconnected = Some((id, words::NO_HUB_TO_DISCONNECT.to_string()));
            cx.notify();
            return;
        };
        let asked = id.clone();
        let disconnecting = asking.client.start(move |client| client.account_disconnect(&asked));
        cx.spawn(async move |this, cx| {
            let answer = disconnecting.recv().await;
            let _ = this.update(cx, |this, cx| {
                match answer {
                    Ok(account) => {
                        this.not_disconnected = None;
                        asking.feed.take_reply(Record::Account(account));
                    }
                    Err(e) => this.not_disconnected = Some((id, words::disconnect_failed(&e))),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

/// Scrolls `scroll` the least that brings `bounds`, where something on it was just laid out, into sight, or as much
/// of it as fits, from its top. Says whether it had to.
fn bring_into_sight(scroll: &ScrollHandle, bounds: Bounds<Pixels>) -> bool {
    let sight = scroll.bounds();
    let by = if bounds.top() < sight.top() || bounds.size.height > sight.size.height {
        sight.top() - bounds.top()
    } else if bounds.bottom() > sight.bottom() {
        sight.bottom() - bounds.bottom()
    } else {
        return false;
    };
    let offset = scroll.offset();
    scroll.set_offset(point(offset.x, (offset.y + by).min(px(0.))));
    true
}
