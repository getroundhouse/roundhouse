//! The pointing hand, over what a press does something to. `docs/spec/look.md` has the rule.

use gpui_kit::component::Disableable;
use gpui_kit::{CursorStyle, HitboxBehavior, IntoElement, ParentElement as _, Styled, canvas, div};

/// Something a press does something to: a button, a tab, a switch, or a row that opens something. The kit gives its
/// buttons the arrow, as a Mac's own have, so each one asks for the hand here.
pub trait Hand: Styled + Sized {
    /// The pointing hand over it.
    #[must_use]
    fn hand(self) -> Self {
        self.cursor_pointer()
    }

    /// The hand only while a press would do something, for what's off without being greyed out, such as a button
    /// that's waiting.
    #[must_use]
    fn hand_while(self, pressable: bool) -> Self {
        if pressable { self.hand() } else { self }
    }
}

impl<T: Styled> Hand for T {}

/// The hand over the element this lies in, while `pressable` says a press would do something, for what can only be
/// pressed when it's laid out a certain way, such as a command that shows more only while it's cut off. That's known
/// once the window has laid everything out, so it's asked as the window paints, which [`Hand`] is too early for. It
/// lies over the element, as large as it is, so the element is `relative`.
pub fn hand_once_laid_out(pressable: impl Fn() -> bool + 'static) -> impl IntoElement {
    let lies_over = canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| {
            if pressable() {
                window.set_cursor_style(CursorStyle::PointingHand, &hitbox);
            }
        },
    );
    div().absolute().inset_0().child(lies_over.size_full())
}

/// Something that can be greyed out.
pub trait Off: Disableable + Styled + Sized {
    /// Greyed out and under the arrow while `off`, and under the hand otherwise. They're set together because the
    /// kit lays a button's own style over its disabled one, so a hand given to a button that's then turned off
    /// would stay.
    #[must_use]
    fn off_while(self, off: bool) -> Self {
        self.disabled(off).hand_while(!off)
    }
}

impl<T: Disableable + Styled> Off for T {}

#[cfg(test)]
mod tests {
    use gpui_kit::component::button::Button;
    use gpui_kit::{CursorStyle, div};

    use super::*;

    #[test]
    fn what_can_be_pressed_asks_for_the_hand() {
        assert_eq!(div().hand().style().mouse_cursor, Some(CursorStyle::PointingHand));
        assert_eq!(Button::new("on").off_while(false).style().mouse_cursor, Some(CursorStyle::PointingHand));
    }

    #[test]
    fn whats_off_is_left_the_cursor_the_kit_gives_it() {
        assert_eq!(Button::new("off").off_while(true).style().mouse_cursor, None);
        assert_eq!(Button::new("waiting").hand_while(false).style().mouse_cursor, None);
    }
}
