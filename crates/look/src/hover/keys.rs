//! The keys that go between what a hover card is for and what's in the card.

use gpui_kit::{App, KeyBinding, actions};

actions!(hover_card, [TabOn, TabBack]);

/// Around what a card that shows on the keyboard's focus is for, and around the card.
pub(super) const KEY_CONTEXT: &str = "HoverCard";

/// Binds Tab and Shift-Tab in a hover card that shows on the keyboard's focus. The card is drawn after the rest of the
/// window, so on their own they'd go past it to whatever comes after what it's for.
pub fn bind_hover_card_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", TabOn, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-tab", TabBack, Some(KEY_CONTEXT)),
    ]);
}
