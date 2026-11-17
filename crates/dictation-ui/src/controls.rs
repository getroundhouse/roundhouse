//! The controls that replace a host's ordinary toolbar until the last words have settled.

use gpui_kit::component::h_flex;
use gpui_kit::{App, Entity, Focusable as _, IntoElement, ParentElement as _, Styled as _, div, px};
use look::Shaped as _;

use crate::Field;

pub const STOP: &str = "dictation-stop";
pub const CANCEL: &str = "dictation-cancel";

impl Field {
    /// The caller keeps its usual toolbar's padding and height. Stop only finishes dictation; it never submits.
    pub fn controls(field: &Entity<Self>, size: look::ControlSize, cx: &App) -> Option<impl IntoElement + use<>> {
        let underway = field.read(cx).underway.as_ref()?;
        let stopping = underway.stopping;
        let (cancel, stop) = (field.downgrade(), field.downgrade());
        let colors = look::colors(cx);
        Some(
            h_flex()
                .w_full()
                .justify_end()
                .gap(look::Space::Small)
                .child(
                    look::button(CANCEL, look::ButtonKind::Borderless, size, "Esc")
                        .accessibility_label("Cancel dictation")
                        .tooltip("Cancel dictation (Esc)")
                        .on_click(move |_, window, cx| {
                            let _ = cancel.update(cx, |field, cx| {
                                window.focus(&field.input.focus_handle(cx), cx);
                                field.cancel(window, cx);
                            });
                        }),
                )
                .child(
                    look::glyph_button(
                        STOP,
                        look::ButtonKind::Borderless,
                        size,
                        div().size(px(9.)).corners(look::Corner::Glyph).bg(colors.brand),
                    )
                    .accessibility_label("Finish dictation")
                    .tooltip("Finish dictation")
                    .off(stopping)
                    .on_click(move |_, window, cx| {
                        let _ = stop.update(cx, |field, cx| {
                            window.focus(&field.input.focus_handle(cx), cx);
                            field.stop(cx);
                        });
                    }),
                ),
        )
    }

    /// A paint-only overlay inside the host's existing field bounds. It stays mounted to finish fading out.
    pub fn edge(&self) -> impl IntoElement + use<> {
        div().absolute().inset_0().child(self.line.clone())
    }
}
