use gpui_kit::component::h_flex;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::{
    App, IntoElement, Keystroke, OperatingSystem, ParentElement as _, RenderOnce, SharedString, Styled as _, Window,
    div, prelude::FluentBuilder as _, px,
};

use super::KeyKind;
use crate::{Corner, EDGE, Shaped as _, Space, Type, Typeset as _, colors};

/// A key to press, as `binding` names it, such as `secondary-f` or `enter`, shown as `kind`. It's read for the system
/// it's drawn on, so `secondary` is Cmd on a Mac and Ctrl elsewhere, and written as that system writes keys.
pub fn key(kind: KeyKind, binding: impl Into<SharedString>) -> Key {
    Key { kind, binding: binding.into() }
}

/// The symbols the kit writes on a Mac for the keys that are held, in the order it writes them: Control, Option, Shift
/// and Command.
const HELD: [char; 4] = ['⌃', '⌥', '⇧', '⌘'];

/// A key to press, which [`key`] makes.
#[derive(IntoElement)]
pub struct Key {
    kind: KeyKind,
    binding: SharedString,
}

impl Key {
    /// The parts `binding` is written in on `os`, with what's held first. On a Mac each held key is a symbol of its
    /// own, so it can be smaller than the key pressed, as they are on its keys, and the key pressed is the last part,
    /// such as `Tab`. Elsewhere the parts are words, and come as one, with what goes between them. A binding that isn't
    /// a key has none.
    pub fn parts(binding: &str, os: OperatingSystem) -> Vec<String> {
        let Ok(keystroke) = Keystroke::parse_with_os(binding, os) else { return Vec::new() };
        // Escape in words, as it's written elsewhere, since the Mac's symbol for it, a circle with an arrow, reads as
        // undo.
        let said = Kbd::format(&keystroke).replace('⎋', "Esc");
        if !cfg!(target_os = "macos") {
            return vec![said];
        }
        let pressed = said.trim_start_matches(HELD);
        if pressed.is_empty() {
            // A held key pressed on its own, such as Shift, is the key pressed.
            return vec![said];
        }
        let held = said[..said.len() - pressed.len()].chars().map(String::from);
        held.chain([pressed.to_string()]).collect()
    }
}

impl RenderOnce for Key {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let parts = Key::parts(&self.binding, cx.platform_info().os);
        match self.kind {
            KeyKind::Plain => parts.concat().into_any_element(),
            KeyKind::Capped => {
                let colors = colors(cx);
                let pressed = parts.len().saturating_sub(1);
                let parts = parts
                    .into_iter()
                    .enumerate()
                    .map(|(ix, part)| div().when(ix < pressed, |held| held.typeset(Type::KeyHeld)).child(part));
                h_flex()
                    .flex_none()
                    .h(px(16.))
                    .px(Space::XSmall)
                    .gap(Space::XSmall)
                    .corners(Corner::InlineControl)
                    .border(EDGE)
                    .border_color(colors.control_edge)
                    .bg(colors.raised)
                    .typeset(Type::Key)
                    .text_color(colors.text_muted)
                    .children(parts)
                    .into_any_element()
            }
        }
    }
}

/// The kit's key for `binding`, as a tooltip shown for a sheet is given it, or none where it isn't a key.
#[cfg(any(test, feature = "testing"))]
pub(crate) fn kit_key(binding: &'static str) -> Option<Kbd> {
    Keystroke::parse(binding).ok().map(Kbd::new)
}
