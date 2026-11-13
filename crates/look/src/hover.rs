//! A hover card, timed the same as every other in the app and its transcript.

use std::time::Duration;

use gpui_kit::component::hover_card::HoverCard;
use gpui_kit::{ElementId, Overflow, Styled as _};

/// How long the pointer rests before a card shows: as long as GPUI waits before any other tooltip in the app shows,
/// so a card doesn't feel slower than one.
const OPENS_AFTER: Duration = Duration::from_millis(500);
/// A card goes as soon as the pointer does, as a tooltip does.
const CLOSES_AFTER: Duration = Duration::ZERO;
/// How long a card with a button in it waits after the pointer leaves what it opened from, so the pointer can cross the
/// gap to the button.
const CROSSING: Duration = Duration::from_millis(300);

/// A hover card with `id`, which shows once the pointer has rested on what it's for as long as before a tooltip, and
/// goes as soon as the pointer does. The kit's look is off, so what it shows draws its own card, such as
/// `look::raised`, or nothing, as the update's details do, which show elsewhere. Where it opens is the caller's.
pub fn hover_card(id: impl Into<ElementId>) -> HoverCard {
    let mut card = HoverCard::new(id).open_delay(OPENS_AFTER).close_delay(CLOSES_AFTER).appearance(false);
    // The kit holds what a card shows inside a box of its own, which would cut off the shadow under the card drawn in
    // it. With the kit's look off, that box is only as big as what's in it, so the shadow is all that reaches past.
    card.style().overflow.x = Some(Overflow::Visible);
    card.style().overflow.y = Some(Overflow::Visible);
    card
}

/// A [`hover_card`] with a button in it, which waits a moment after the pointer leaves what it's for, long enough for
/// the pointer to cross to the button.
pub fn hover_card_with_a_button(id: impl Into<ElementId>) -> HoverCard {
    hover_card(id).close_delay(CROSSING)
}
