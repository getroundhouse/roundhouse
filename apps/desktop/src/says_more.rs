//! What a hover card says that a tooltip has no room for, since a tooltip holds a few words, as
//! `docs/spec/components.md` says: a path, what went wrong, or how something works. Each is raised as the app's other
//! cards are, as wide as its context card, and shows once the pointer has rested on what it's for as long as before a
//! tooltip.

use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Anchor, AnyElement, App, IntoElement, ParentElement as _, StyleRefinement, Styled as _, div};
use look::{HoverCard, Region, Typeset as _};

/// `trigger`, with the card `card` makes, which `hover` shows from the trigger's `anchor`, and the trigger laid out by
/// `style`, so it can still be cut short. A card with a button in it is a `look::hover_card_with_a_button`, which
/// waits for the pointer to cross to it.
pub(crate) fn on_hover(
    hover: HoverCard,
    trigger: impl IntoElement,
    anchor: Anchor,
    style: StyleRefinement,
    card: impl FnOnce(&mut App) -> AnyElement + 'static,
) -> HoverCard {
    hover.anchor(anchor).trigger_style(style).trigger(trigger).content(move |_, _, cx| card(cx))
}

/// The card: what it's about, if there's a heading, then what it says, in muted detail text, each a paragraph.
pub(crate) fn card(region: Region, heading: Option<String>, says: Vec<String>, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let card = surface(region).when_some(heading, |card, heading| card.heading(look::card_heading(heading)));
    says.into_iter()
        .fold(card, |card, said| {
            card.under_heading(div().typeset(look::Type::Detail).text_color(colors.text_muted).child(said))
        })
        .into_any_element()
}

/// The card for a path, in code's type so it reads as one, wrapped where it's longer than the card, with the button
/// that copies it.
pub(crate) fn path_card(region: Region, path: String, copy_id: &'static str, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let text = div().flex_1().min_w_0().typeset(look::Type::Code).text_color(colors.text_muted).child(path.clone());
    let copy = look::copy_button(copy_id, path, "Copy path", cx);
    surface(region).child(h_flex().items_start().gap(look::Space::Small).child(text).child(copy)).into_any_element()
}

/// A card over the rest, as wide as the chat's context card.
fn surface(region: Region) -> look::Card {
    look::card(look::CardKind::Over, look::CardWidth::Small).region(region)
}
