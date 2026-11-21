//! Setting up conversation mode, as `docs/spec/chats.md` says: until dictation and speech are both ready, turning the
//! mode on opens a card that says how each stands, turns on what isn't, and starts the mode once both are ready. What
//! each part's state is called, and what turning on says, are `client-core`'s, so every client says the same.

use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use client_core::conversation::{Needs, Part, Readiness};
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::popover::Popover;
use gpui_kit::component::progress::Progress;
use gpui_kit::component::{Icon, Selectable, h_flex, v_flex};
use gpui_kit::{
    Anchor, AnyElement, App, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    Styled as _, TestSupportExt as _, Window, div,
};
use look::Region;
use protocol::model::SessionId;

use super::WorkspacePane;
use crate::settings::{Choice, change};

/// The card's buttons, for tests to find them by.
pub const CONVERSATION_TURN_ON: &str = "conversation-turn-on";
pub const CONVERSATION_NOT_NOW: &str = "conversation-not-now";
pub const CONVERSATION_RETRY: &str = "conversation-retry";
pub const CONVERSATION_CARD: &str = Region::ConversationCard.id();
/// How far each part's download has got, while it's downloading.
pub const DICTATION_PROGRESS: &str = "conversation-dictation-progress";
pub const SPEECH_PROGRESS: &str = "conversation-speech-progress";

/// What the card says the mode is, and what it needs.
const SAYS: &str = "Talk with the agent and hear its replies. It needs dictation and speech, downloaded once, which \
                    run on this Mac, so nothing you say leaves it.";

/// A chat waiting on dictation and speech to start its conversation, while its card is open.
pub(super) struct SettingUp {
    chat: SessionId,
    /// Held by the card while it's drawn, so the mode starts by itself only while the card shows, and not once its
    /// workspace or tab has gone from view.
    shown: Rc<()>,
    /// Which card this is, so a card opened again doesn't keep what an earlier one held.
    number: usize,
}

/// The number of the next card.
static NEXT: AtomicUsize = AtomicUsize::new(0);

/// What the card holds while it's drawn.
struct Shown(#[expect(dead_code, reason = "held only to be counted")] Rc<()>);

impl WorkspacePane {
    /// Whether the chat showing is waiting on dictation and speech to start its conversation, with the card open.
    fn setting_up_here(&self) -> bool {
        self.setting_up.as_ref().is_some_and(|setting_up| Some(&setting_up.chat) == self.active.as_ref())
    }

    /// Opens the card for the chat showing, as turning the mode on does before dictation and speech are ready, where
    /// it can open: from the mode's button, while the mode is off and the button is there, or from Take over and
    /// Take back, while the conversation is on another device and this Mac can hear. Elsewhere, nothing happens.
    pub(super) fn set_up(&mut self, cx: &mut Context<Self>) {
        let Some(chat) = self.active.clone() else { return };
        let card_shows = self.showing() == super::Showing::Chat
            && match self.conversations.read(cx).get(&chat) {
                None => crate::conversing::offered(cx),
                Some(conversation) => !conversation.is_here() && crate::dictating::runs_here(cx),
            };
        if card_shows {
            self.setting_up =
                Some(SettingUp { chat, shown: Rc::new(()), number: NEXT.fetch_add(1, Ordering::Relaxed) });
            cx.notify();
        }
    }

    /// Starts the mode in the chat whose card is showing, once dictation and speech are both ready, as the card says
    /// it will. A card for another chat than the one showing has closed.
    pub(super) fn start_once_ready(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.setting_up.is_some() && !self.setting_up_here() {
            self.setting_up = None;
        }
        let shows = self.setting_up.as_ref().is_some_and(|setting_up| Rc::strong_count(&setting_up.shown) > 1);
        if shows && crate::conversing::needs(cx).ready() {
            self.setting_up = None;
            self.start_conversation(window, cx);
        }
        cx.notify();
    }

    /// `trigger`, such as the mode's button or Take over, with the card opening from it while the chat showing is
    /// waiting on dictation and speech.
    pub(super) fn with_setup_card(
        &self,
        trigger: impl Selectable + IntoElement + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pane = cx.entity().downgrade();
        Popover::new("conversation-setup")
            .anchor(Anchor::BottomRight)
            .appearance(false)
            .open(self.setting_up_here())
            .on_open_change(move |open, _, cx| {
                if !open {
                    let _ = pane.update(cx, |pane, cx| {
                        pane.setting_up = None;
                        cx.notify();
                    });
                }
            })
            .trigger(trigger)
            .content({
                let pane = cx.entity().downgrade();
                let shown = self.setting_up.as_ref().map(|setting_up| (setting_up.number, setting_up.shown.clone()));
                move |_, window, cx| {
                    if let Some((number, shown)) = &shown {
                        let key = ("conversation-card-shown", *number);
                        window.use_keyed_state(key, cx, |_, _| Shown(shown.clone()));
                    }
                    card(&crate::conversing::needs(cx), pane.clone(), cx)
                }
            })
            .into_any_element()
    }
}

/// The card: what the mode needs, how each part stands, and what can be done about it.
fn card(needs: &Needs, pane: gpui_kit::WeakEntity<WorkspacePane>, cx: &mut App) -> AnyElement {
    let colors = look::colors(cx);
    let rows = Part::ALL.into_iter().map(|part| row(part, needs.of(part), cx));
    let turn_on = needs.turn_on().map(|label| {
        let not_on = needs.not_on();
        look::button(CONVERSATION_TURN_ON, look::ButtonKind::Main, look::ControlSize::Regular, label)
            .on_click(move |_, _, cx| turn_on(&not_on, cx))
            .into_any_element()
    });
    let retry = needs.failed().map(|part| {
        look::button(CONVERSATION_RETRY, look::ButtonKind::Outlined, look::ControlSize::Regular, "Retry")
            .on_click(move |_, _, cx| retry(part, cx))
            .into_any_element()
    });
    let not_now = look::button(CONVERSATION_NOT_NOW, look::ButtonKind::Outlined, look::ControlSize::Regular, "Not now")
        .on_click(move |_, _, cx| {
            let _ = pane.update(cx, |pane, cx| {
                pane.setting_up = None;
                cx.notify();
            });
        });
    look::card(look::CardKind::Over, look::CardWidth::Regular)
        .region(Region::ConversationCard)
        .heading(look::card_heading("Conversation mode"))
        .under_heading(div().text_color(colors.text_muted).child(SAYS))
        .child(v_flex().w_full().gap(look::Space::Medium).children(rows))
        .child(h_flex().gap(look::Space::Small).children(turn_on).children(retry).child(not_now))
        .into_any_element()
}

/// One part's row: what it is, how big, how it stands, and how far its download has got.
fn row(part: Part, readiness: &Readiness, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let (icon, color) = match readiness {
        Readiness::Ready => (Lucide::CircleCheck, colors.done),
        Readiness::Failed(_) => (Lucide::TriangleAlert, colors.warning),
        Readiness::Downloading(_) => (Lucide::Download, colors.text),
        Readiness::Preparing => (Lucide::LoaderCircle, colors.text),
        Readiness::NotOn => (Lucide::CircleDashed, colors.text_faint),
    };
    let size = match part {
        Part::Dictation => crate::dictating::size(),
        Part::Speech => crate::conversing::size(),
    };
    let says = SharedString::from(readiness.says());
    let line = h_flex()
        .w_full()
        .gap(look::Space::Small)
        .child(Icon::new(icon).text_color(color).flex_none())
        .child(div().text_color(colors.text).child(part.name()))
        .child(div().text_color(colors.text_faint).child(size))
        .child(div().flex_1())
        .child(
            div()
                .text_color(if *readiness == Readiness::Ready { colors.done } else { colors.text_muted })
                .child(says.clone()),
        );
    let bar = match readiness {
        Readiness::Downloading(Some(done)) => {
            let id = match part {
                Part::Dictation => DICTATION_PROGRESS,
                Part::Speech => SPEECH_PROGRESS,
            };
            let bar =
                Progress::new(format!("{id}-bar")).value(done * 100.).color(colors.text).accessibility_label(says);
            Some(div().id(id).test_support().w_full().child(bar))
        }
        _ => None,
    };
    v_flex().w_full().gap(look::Space::XSmall).child(line).children(bar).into_any_element()
}

/// Turns on each of `parts`, as the person said to, in Settings too.
pub(super) fn turn_on(parts: &[Part], cx: &mut App) {
    change(cx, |settings| {
        for part in parts {
            match part {
                Part::Dictation => settings.dictation = Choice::On,
                Part::Speech => settings.speech = Choice::On,
            }
        }
    });
}

/// Tries getting `part` ready again, after it failed.
pub(super) fn retry(part: Part, cx: &mut App) {
    match part {
        Part::Dictation => crate::dictating::retry(cx),
        Part::Speech => crate::conversing::retry(cx),
    }
}
