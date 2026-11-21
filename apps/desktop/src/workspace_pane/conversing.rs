//! Conversation mode in the pane, as `docs/spec/chats.md` says: the button that turns it on, beside the microphone,
//! and the bar that takes the composer's place while it's on. The conversations themselves are the hub view's, followed
//! from the records whichever workspace shows, as `crate::conversing` says. What the bar shows, and what's read, are
//! `client-core`'s, the same for every client, so this only draws them and passes keys on.
//!
//! The composer's text field still has the keys while the bar shows, so holding Space hears what's said as dictating
//! does, into the field, and letting go sends it, said out loud, leaving what was typed there before. Any other key
//! typed turns the mode off and brings the composer back with it typed there.

use client_core::Reply;
use client_core::conversation::{Conversation, HereNow, Needs, Readiness, Shows, Stage};
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{Icon, IconName, h_flex};
use gpui_kit::{
    AnyElement, App, Context, Focusable as _, InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
};
use look::{InRegion as _, Region, Shaped as _, Typeset as _};
use protocol::model::SessionId;
use protocol::prompt::Prompt;

use super::composer::Slot;
use super::setting_up::{self, CONVERSATION_RETRY, CONVERSATION_TURN_ON};
use super::{PaneEvent, WorkspacePane};
use crate::ConversationMode;

/// The button that turns the mode on, and the bar's, for tests to find them by.
pub const CONVERSATION_MODE: &str = "conversation-mode";
pub const CONVERSATION_TYPE: &str = "conversation-type";
pub const CONVERSATION_END: &str = "conversation-end";
pub const CONVERSATION_STOP: &str = "conversation-stop";
pub const TAKE_OVER: &str = "conversation-take-over";
pub const CONVERSATION_BAR: &str = Region::ConversationBar.id();

impl WorkspacePane {
    /// The conversation of the chat showing, while it's on this Mac.
    fn conversation_here<'a>(&self, cx: &'a App) -> Option<&'a Conversation> {
        self.conversation_of(self.active.as_ref()?, cx).filter(|conversation| conversation.is_here())
    }

    /// The conversation of the chat `session`, while it's in conversation mode.
    fn conversation_of<'a>(&self, session: &SessionId, cx: &'a App) -> Option<&'a Conversation> {
        self.conversations.read(cx).get(session)
    }

    /// Turns conversation mode on in the chat showing, with the conversation on this Mac, or moves it here, as the button,
    /// `⇧⌘Space`, Take over and Take back do, once dictation and speech are ready here. Until then, the card that sets
    /// them up opens instead, and starts the mode once they are.
    pub fn start_conversation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.active.clone() else { return };
        if !crate::conversing::needs(cx).ready() {
            self.set_up(cx);
            return;
        }
        window.focus(&self.composer.focus_handle(cx), cx);
        let asking = self.client.start(move |client| client.session_conversation_start(&id).map(drop));
        Self::say_if_refused(asking, "turn conversation mode on", cx);
    }

    /// Turns conversation mode off in the chat showing, as Type, ✕ and typing do, once the voice has stopped and the
    /// hub has been told how far it got. The bar stays until it is, so a refusal says why.
    fn end_conversation(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.active.clone() else { return };
        if let Some(ending) = self.conversations.update(cx, |conversations, _| conversations.end(&id)) {
            Self::say_if_refused(ending, "turn conversation mode off", cx);
        }
    }

    /// Says why the hub didn't do `what`, once `asking` answers, if it didn't.
    fn say_if_refused(asking: Reply<()>, what: &'static str, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            if let Err(e) = asking.recv().await {
                let couldnt = client_core::refusals::couldnt(what, &e);
                let _ = this.update(cx, |_, cx| cx.emit(PaneEvent::Error(couldnt)));
            }
        })
        .detach();
    }

    /// Dictation began, so in conversation mode the person is talking: what's being read is talked over. What the
    /// composer held then isn't part of what they say.
    pub(super) fn talking(&mut self, cx: &mut Context<Self>) {
        if self.dictation_chat != self.active {
            return;
        }
        if let (Some(conversation), Some(id)) = (self.conversation_here(cx), self.active.clone()) {
            conversation.talk();
            self.typed_before_talking = Some((id, self.composer.read(cx).text().to_string()));
        }
    }

    /// Sends what was heard while Space was held, said out loud, once the last of it has gone into the field, and puts
    /// back what the composer held before. Returns whether it was said in conversation mode, rather than dictated.
    pub(super) fn send_said(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((talking_in, before)) = self.typed_before_talking.take() else { return false };
        // Another chat shows now, whose composer holds its own words: nothing of them was said.
        if self.active.as_ref() != Some(&talking_in) {
            if let Some(conversation) = self.conversation_of(&talking_in, cx) {
                conversation.let_go(false);
            }
            return false;
        }
        let now = self.composer.read(cx).text().to_string();
        let said = said_since(&before, &now).trim().to_string();
        let transcript = self.transcript().filter(|_| !said.is_empty());
        let sent =
            transcript.is_some_and(|t| t.update(cx, |t, cx| t.send_prompt_with(Prompt::new(said).said_out_loud(), cx)));
        if let Some(conversation) = self.conversation_here(cx) {
            conversation.let_go(sent);
        }
        if sent {
            self.composer.update(cx, |composer, cx| composer.set_value(before, window, cx));
            self.keep_typed(true, cx);
        }
        true
    }

    /// The button between the context's ring and the microphone that turns the mode on, with the card that sets it up
    /// opening from it until dictation and speech are ready.
    pub(super) fn conversation_button(&self, cx: &mut Context<Self>) -> Slot {
        let button = look::icon_button(CONVERSATION_MODE, look::ButtonKind::Borderless, Slot::SIZE, Lucide::AudioLines)
            .accessibility_label("Conversation mode")
            .tooltip_with_action("Conversation mode", &ConversationMode, None)
            .on_click(cx.listener(|t, _, window, cx| t.start_conversation(window, cx)));
        Slot::holding(self.with_setup_card(button, cx))
    }

    /// The bar that takes the composer's place while the mode is on, or `None` while it's off.
    pub(super) fn conversation_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?.clone();
        let tail = self.transcript()?.read(cx).log().tail().clone();
        let field = self.dictation.read(cx);
        let hearing = field.is_dictating() && self.dictation_chat == self.active;
        let needs = crate::conversing::needs(cx);
        let conversation = self.conversation_of(self.active.as_ref()?, cx)?;
        let here = HereNow { hearing, reading: conversation.reading_a_reply(), needs };
        let shows = conversation.shows(&session, &tail, &here);
        let words = shows.says().map(|words| says(words, cx));
        Some(match shows {
            Shows::Composer => return None,
            Shows::Here { stage, needs } => self.bar_here(&stage, &needs, words, cx),
            Shows::Elsewhere { .. } => {
                let mut actions: Vec<AnyElement> = shows
                    .on()
                    .map(|on| div().text_color(look::colors(cx).text_faint).child(on).into_any_element())
                    .into_iter()
                    .collect();
                actions.extend(self.take_over("Take over", cx));
                bar(Lucide::AudioLines, words?, actions, cx).into_any_element()
            }
            Shows::MovedTo { .. } => {
                let take_back = self.take_over("Take back", cx).into_iter().collect();
                bar(Lucide::AudioLines, words?, take_back, cx).into_any_element()
            }
        })
    }

    /// The bar while the conversation is here, which has the keys: holding Space talks, once dictation and speech are
    /// ready, and another key types. `words` are what it says, where every client says the same.
    fn bar_here(&self, stage: &Stage, needs: &Needs, words: Option<AnyElement>, cx: &mut Context<Self>) -> AnyElement {
        let colors = look::colors(cx);
        let type_ = look::button(CONVERSATION_TYPE, look::ButtonKind::Borderless, look::ControlSize::Small, "Type")
            .icon(Icon::new(Lucide::Keyboard))
            .on_click(cx.listener(|t, _, window, cx| t.leave_to_type(None, window, cx)));
        let end = || {
            look::icon_button(CONVERSATION_END, look::ButtonKind::Borderless, look::ControlSize::Small, IconName::Close)
                .accessibility_label("Leave conversation mode")
                .tooltip("Leave conversation mode")
                .on_click(cx.listener(|t, _, _, cx| t.end_conversation(cx)))
                .into_any_element()
        };
        // How to talk on a Mac, where the bar says nothing more.
        let says = words.unwrap_or_else(|| {
            h_flex()
                .gap(look::Space::Small)
                .child("Hold")
                .child(look::key(look::KeyKind::Capped, "space"))
                .child("to talk")
                .into_any_element()
        });
        let (icon, says, actions) = match stage {
            Stage::Hearing => self.hearing(cx),
            Stage::Speaking => {
                let says = h_flex()
                    .gap(look::Space::Small)
                    .child(says)
                    .child(div().text_color(colors.text_faint).child("· hold Space to talk over it"));
                let stop =
                    look::button(CONVERSATION_STOP, look::ButtonKind::Borderless, look::ControlSize::Small, "Stop Esc")
                        .accessibility_label("Stop speaking")
                        .on_click(cx.listener(|t, _, _, cx| t.stop_voice(cx)))
                        .into_any_element();
                (Lucide::AudioLines, says.into_any_element(), vec![stop, end()])
            }
            Stage::Working { .. } => {
                let stop =
                    look::button(CONVERSATION_STOP, look::ButtonKind::Borderless, look::ControlSize::Small, "Stop")
                        .accessibility_label("Stop the agent")
                        .on_click(cx.listener(|t, _, _, cx| {
                            t.stop_voice(cx);
                            t.stop_agent(cx);
                        }))
                        .into_any_element();
                (Lucide::AudioLines, says, vec![type_.into_any_element(), stop])
            }
            Stage::Waiting => Self::waiting(needs, says, type_.into_any_element(), end()),
        };
        let surface = bar(icon, says, actions, cx);
        // Holding Space, or Option-Space, talks only once dictation and speech are ready, and does nothing before.
        let surface = if needs.ready() { dictation_ui::Field::keys(surface, &self.dictation) } else { surface };
        let surface = surface
            .track_focus(&self.composer.focus_handle(cx))
            .capture_key_down(cx.listener(Self::key_in_the_bar))
            .child(self.dictation.read(cx).edge());
        surface.into_any_element()
    }

    /// What the bar says while Space is held: what's been heard, and what's being heard, which letting go sends.
    fn hearing(&self, cx: &mut Context<Self>) -> (Lucide, AnyElement, Vec<AnyElement>) {
        let field = self.dictation.read(cx);
        let now = self.composer.read(cx).text().to_string();
        let before = self.typed_before_talking.as_ref().map_or("", |(_, before)| before.as_str());
        let heard = said_since(before, &now).trim().to_string();
        let words = h_flex().gap(look::Space::Small).child(heard).children(field.live_line(cx));
        let let_go = div().text_color(look::colors(cx).text_faint).child("Let go to send").into_any_element();
        let esc = look::button(CONVERSATION_STOP, look::ButtonKind::Borderless, look::ControlSize::Small, "Esc")
            .accessibility_label("Drop what was said")
            .on_click(cx.listener(|t, _, window, cx| t.drop_said(window, cx)))
            .into_any_element();
        (Lucide::AudioLines, words.into_any_element(), vec![let_go, esc])
    }

    /// The bar's icon and actions while it waits for the person to talk, or for dictation and speech to be ready, as
    /// when the conversation came here from another device first: Retry once one failed, and Turn on for what isn't.
    fn waiting(
        needs: &Needs,
        says: AnyElement,
        type_: AnyElement,
        end: AnyElement,
    ) -> (Lucide, AnyElement, Vec<AnyElement>) {
        if let Some(part) = needs.failed() {
            let retry =
                look::button(CONVERSATION_RETRY, look::ButtonKind::Borderless, look::ControlSize::Small, "Retry")
                    .on_click(move |_, _, cx| setting_up::retry(part, cx))
                    .into_any_element();
            return (Lucide::TriangleAlert, says, vec![retry, end]);
        }
        if needs.turn_on().is_some() {
            let not_on = needs.not_on();
            let turn_on =
                look::button(CONVERSATION_TURN_ON, look::ButtonKind::Borderless, look::ControlSize::Small, "Turn on")
                    .on_click(move |_, _, cx| setting_up::turn_on(&not_on, cx))
                    .into_any_element();
            return (Lucide::AudioLines, says, vec![turn_on, type_, end]);
        }
        let getting_ready =
            [&needs.dictation, &needs.speech].into_iter().any(|readiness| *readiness != Readiness::Ready);
        (if getting_ready { Lucide::Download } else { Lucide::AudioLines }, says, vec![type_, end])
    }

    /// Take over or Take back, which moves the conversation here, with the card that sets up the mode opening from it
    /// until dictation and speech are ready, or `None` where this device can't hear, so the mode can't come here.
    fn take_over(&self, label: &'static str, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !crate::dictating::runs_here(cx) {
            return None;
        }
        let button = look::button(TAKE_OVER, look::ButtonKind::Borderless, look::ControlSize::Small, label)
            .on_click(cx.listener(|t, _, window, cx| t.start_conversation(window, cx)));
        Some(self.with_setup_card(button, cx))
    }

    /// A key while the bar has them. Space held is dictation's, which talks once it hears. Esc stops the voice. Anything
    /// else typed turns the mode off, with it typed into the composer.
    fn key_in_the_bar(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        if keystroke.key == "space" && !keystroke.modifiers.modified() {
            return;
        }
        if keystroke.key == "escape" && self.conversation_here(cx).is_some_and(Conversation::reading) {
            self.stop_voice(cx);
            cx.stop_propagation();
            return;
        }
        let typed =
            keystroke.key_char.as_ref().filter(|_| !keystroke.modifiers.control && !keystroke.modifiers.platform);
        if let Some(typed) = typed.cloned() {
            self.leave_to_type(Some(typed.into()), window, cx);
            cx.stop_propagation();
        }
    }

    /// Turns the mode off and brings the composer back, with `typed` in it, as Type and typing do.
    fn leave_to_type(&mut self, typed: Option<SharedString>, window: &mut Window, cx: &mut Context<Self>) {
        self.end_conversation(cx);
        window.focus(&self.composer.focus_handle(cx), cx);
        if let Some(typed) = typed {
            self.composer.update(cx, |composer, cx| composer.insert(typed, window, cx));
        }
    }

    /// Stops what the voice is reading, as Stop and Esc do while it speaks.
    fn stop_voice(&mut self, cx: &mut Context<Self>) {
        if let Some(conversation) = self.conversation_here(cx) {
            conversation.stop_voice();
        }
        cx.notify();
    }

    /// Drops what was said while Space is held, sending nothing, as Esc does.
    fn drop_said(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dictation.update(cx, |field, cx| field.cancel(window, cx));
    }
}

/// What's in `now` that wasn't in `before`, the composer's text as dictating began, wherever its cursor put it.
fn said_since<'a>(before: &str, now: &'a str) -> &'a str {
    let same =
        |a: &mut dyn Iterator<Item = (char, char)>| a.take_while(|(x, y)| x == y).map(|(c, _)| c.len_utf8()).sum();
    let start: usize = same(&mut before.chars().zip(now.chars()));
    let room = before.len().min(now.len()) - start;
    let end: usize = same(&mut before.chars().rev().zip(now.chars().rev())).min(room);
    &now[start..now.len() - end]
}

/// What the bar says, in a line.
fn says(words: String, cx: &Context<WorkspacePane>) -> AnyElement {
    div().text_color(look::colors(cx).text).truncate().child(words).into_any_element()
}

/// The bar: an icon, what it says, and its actions at its right end.
fn bar(
    icon: Lucide,
    says: AnyElement,
    actions: Vec<AnyElement>,
    cx: &Context<WorkspacePane>,
) -> impl IntoElement + InteractiveElement + ParentElement {
    let colors = look::colors(cx);
    h_flex()
        .region(Region::ConversationBar)
        .role(Role::Group)
        .aria_label("Conversation mode")
        .relative()
        .w_full()
        .gap(look::Space::Medium)
        .px(look::Space::Large)
        .py(look::Space::Medium)
        .corners(look::Corner::Card)
        .border(look::EDGE)
        .border_color(colors.control_edge)
        .bg(colors.inset)
        .typeset(look::Type::Body)
        .child(Icon::new(icon).text_color(colors.text_muted))
        .child(div().flex_1().min_w_0().child(says))
        .child(h_flex().flex_none().gap(look::Space::Small).children(actions))
}

#[cfg(test)]
mod tests {
    use super::said_since;

    /// What was said is what dictating put in, wherever the cursor was, and none of what was typed before.
    #[test]
    fn what_was_said_is_only_what_went_in() {
        assert_eq!(said_since("", "what's the status"), "what's the status");
        assert_eq!(said_since("draft", "draft what's the status"), " what's the status");
        assert_eq!(said_since("a draft", "a what's the status draft"), "what's the status ");
        assert_eq!(said_since("aa", "aaa"), "a");
        assert_eq!(said_since("café", "café olé"), " olé");
        assert_eq!(said_since("draft", "draft"), "");
    }
}
