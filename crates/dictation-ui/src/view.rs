//! The shared live line and key handlers. A host can use both without drawing any microphone, as question answers do.

use gpui_kit::Focusable as _;
use gpui_kit::{
    App, Entity, InteractiveElement, IntoElement, ParentElement as _, Role, StatefulInteractiveElement as _, Styled,
    TestSupportExt as _, div,
};
use look::Typeset as _;

use crate::recording::Started;
use crate::{CancelDictation, Field, HoldToDictate, keys};

pub const LIVE_LINE: &str = "dictation-live";

impl Field {
    /// Gives a fresh wrapper around the input its dictation context and handlers. Keep other key contexts on their
    /// own ancestors, since an element has one context. A microphone button is optional.
    pub fn keys<E: InteractiveElement>(surface: E, field: &Entity<Self>) -> E {
        let (hold, cancel, down) = (field.downgrade(), field.downgrade(), field.downgrade());
        surface
            .key_context(keys::CONTEXT)
            .on_action(move |_: &HoldToDictate, window, cx| {
                let handled = hold
                    .update(cx, |field, cx| {
                        if !field.input.focus_handle(cx).is_focused(window)
                            || (!field.is_dictating() && !field.can_dictate(cx))
                        {
                            return false;
                        }
                        keys::held(cx);
                        field.start(Started::Key, window, cx);
                        true
                    })
                    .unwrap_or(false);
                if !handled {
                    cx.propagate();
                }
            })
            .on_action(move |_: &CancelDictation, window, cx| {
                let handled = cancel
                    .update(cx, |field, cx| {
                        if !field.is_dictating() {
                            return false;
                        }
                        field.cancel(window, cx);
                        true
                    })
                    .unwrap_or(false);
                if !handled {
                    cx.propagate();
                }
            })
            .capture_key_down(move |event, window, cx| {
                let _ = down.update(cx, |field, cx| field.key_down(event, window, cx));
            })
    }

    /// A status for screen readers as well as a muted line. The host supplies the inset that matches its input size.
    pub fn live_line(&self, cx: &App) -> Option<impl IntoElement + Styled + use<>> {
        let text = self.underway.as_ref()?.live.as_str();
        if text.is_empty() {
            return None;
        }
        Some(
            div()
                .id(LIVE_LINE)
                .role(Role::Status)
                .aria_label(text.to_string())
                .test_support()
                .typeset(look::Type::Body)
                .text_color(look::colors(cx).text_muted)
                .child(text.to_string()),
        )
    }
}
