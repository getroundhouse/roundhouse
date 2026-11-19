use super::EmptyKind;
use super::nothing::ROOM;
use crate::colors::Colors;
use crate::space::Space;
use crate::spec_table;

#[test]
fn the_specs_table_has_every_kind_of_empty_pane_in_order() {
    let table: Vec<[String; 2]> = spec_table::rows("**Every kind of empty pane, by its name.**");

    let kinds: Vec<[String; 2]> = EmptyKind::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(
        table, kinds,
        "docs/spec/components.md's table of kinds of empty pane, against crates/look/src/empty_states"
    );
}

/// What's missing is the brand's, and what failed is an error, so the mark's colour says which.
#[test]
fn the_mark_is_copper_over_what_s_missing_and_red_over_what_failed() {
    let colors = Colors::dark();

    assert_eq!(EmptyKind::Missing.mark(&colors), colors.brand);
    assert_eq!(EmptyKind::Failed.mark(&colors), colors.error);
}

/// A line saying there's nothing to show keeps the same room at its sides wherever it is.
#[test]
fn nothing_to_show_keeps_24px_at_its_sides() {
    assert_eq!(ROOM, Space::XLarge);
}
