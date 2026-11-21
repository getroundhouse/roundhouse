//! What the keys do to the chat showing from what's around it: Page Up and Page Down scroll it from the composer, or a
//! question in its place, which have little use for them, so the chat can be read back while a prompt is written,
//! `Option-Up` and `Option-Down` step it between its prompts, from there and from the chat, and Escape goes back to the
//! composer from the chat, once Find's bar, if it's open over the chat, has closed. In the chat itself, the keys scroll
//! it as `transcript` takes them.

use gpui_kit::{Context, Div, InteractiveElement as _, Window};

use transcript::{Transcript, Way};

use super::WorkspacePane;
use crate::{BackToComposer, ChatPageDown, ChatPageUp, StepBack, StepOn};

/// Where the keys that reach the chat from around it are bound: the chat, its composer and a question in its place.
pub(crate) const CHAT_KEY_CONTEXT: &str = "Chat";

impl WorkspacePane {
    /// `chat`, which holds the chat showing, its composer and a question in its place, with the keys that reach the
    /// chat from them, and the composer from the chat.
    pub(super) fn with_chat_keys(chat: Div, cx: &mut Context<Self>) -> Div {
        chat.key_context(CHAT_KEY_CONTEXT)
            .on_action(cx.listener(|pane, _: &ChatPageUp, _, cx| {
                if let Some(transcript) = pane.transcript() {
                    transcript.update(cx, |t, cx| t.page_up(cx));
                }
            }))
            .on_action(cx.listener(|pane, _: &ChatPageDown, _, cx| {
                if let Some(transcript) = pane.transcript() {
                    transcript.update(cx, |t, cx| t.page_down(cx));
                }
            }))
            .on_action(cx.listener(|pane, _: &StepBack, _, cx| {
                if let Some(transcript) = pane.transcript() {
                    transcript.update(cx, |transcript, cx| transcript.step(Way::Back, cx));
                }
            }))
            .on_action(cx.listener(|pane, _: &StepOn, _, cx| {
                if let Some(transcript) = pane.transcript() {
                    transcript.update(cx, |transcript, cx| transcript.step(Way::On, cx));
                }
            }))
            .on_action(cx.listener(|pane, _: &BackToComposer, window, cx| pane.back_to_composer(window, cx)))
    }

    /// Goes back to the composer from the chat, or closes Find's bar, if it's open over the chat, which puts focus
    /// back there too, as Escape in the bar does.
    fn back_to_composer(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.transcript().filter(|transcript| transcript.read(cx).is_finding()) {
            Some(finding) => finding.update(cx, Transcript::close_find),
            None => self.focus_composer(window, cx),
        }
    }
}
