use super::{CardKind, CardWidth};
use crate::spec_table;

#[test]
fn the_specs_table_has_every_kind_of_card_in_order() {
    let table: Vec<[String; 1]> = spec_table::rows("**Every kind of card, by its name.**");

    let kinds: Vec<[String; 1]> = CardKind::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(table, kinds, "docs/spec/components.md's table of kinds of card, against crates/look/src/cards");
}

#[test]
fn the_specs_table_has_every_width_of_card_in_order() {
    let table: Vec<[String; 2]> = spec_table::rows("**Every width of a card, by its name.**");

    let widths: Vec<[String; 2]> = CardWidth::ALL.iter().map(|width| width.written()).collect();
    assert_eq!(table, widths, "docs/spec/components.md's table of a card's widths, against crates/look/src/cards");
}

/// Only what opens over the rest casts the shadow, and what's raised has the edge, so a card in the chat is a fill
/// of its own, as a call's block is.
#[test]
fn only_a_card_over_the_rest_casts_the_shadow_and_none_in_the_chat_has_an_edge() {
    let shadowed: Vec<CardKind> = CardKind::ALL.iter().copied().filter(|kind| kind.casts_a_shadow()).collect();
    let edged: Vec<CardKind> = CardKind::ALL.iter().copied().filter(|kind| kind.edged()).collect();

    assert_eq!(shadowed, [CardKind::Over]);
    assert_eq!(edged, [CardKind::Over, CardKind::InPlace]);
}
