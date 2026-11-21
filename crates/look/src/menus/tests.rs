use super::menu::{Entry, in_order};
use super::menu_row;
use super::row::Does;
use super::style::{AROUND, CORNER, ROW, ROW_CORNER};
use crate::Input;
use crate::control_sizes::ControlSize;

#[test]
fn a_menus_rows_are_as_tall_as_a_dialogs_buttons() {
    assert_eq!(ROW.height(), ControlSize::Large.height());
}

#[test]
fn a_rows_corners_sit_evenly_inside_the_menus() {
    assert_eq!(CORNER.radius() - AROUND.size(), ROW_CORNER.radius());
}

#[test]
fn the_row_under_the_pointer_is_washed_as_a_borderless_button_is() {
    let colors = crate::colors::Colors::dark();
    let style = super::style(&colors);
    let button = crate::theme::kit_colors(&colors)
        .into_iter()
        .find(|(key, _)| *key == "button.ghost.hover.background")
        .map(|(_, value)| value);
    assert_eq!(style.hover.map(crate::theme::hex), button);
}

#[test]
fn the_specs_table_has_every_kind_of_note_in_order() {
    let table: Vec<[String; 1]> = crate::spec_table::rows("**Every kind of note at a menu's row's end, by its name.**");

    let kinds: Vec<[String; 1]> = super::NoteKind::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(table, kinds, "docs/spec/components.md's table of kinds of note, against crates/look/src/menus");
}

#[test]
fn under_touch_the_rows_that_copy_come_first_and_with_a_pointer_stay_where_theyre_put() {
    let link = || {
        let (open, copy, go) = (menu_row("Open on GitHub"), menu_row("Copy link").copies(), menu_row("Go to it"));
        vec![Entry::Row(Box::new(open)), Entry::Row(Box::new(copy)), Entry::Separator, Entry::Row(Box::new(go))]
    };
    // Each entry, as whether it's a row that copies, or none where it's a line.
    let copying = |entries: Vec<Entry>| -> Vec<Option<bool>> {
        let copies = |entry: &Entry| match entry {
            Entry::Row(row) => Some(row.does == Does::Copies),
            Entry::Separator | Entry::Label(_) => None,
        };
        entries.iter().map(copies).collect()
    };

    assert_eq!(copying(in_order(link(), Input::Touch)), [Some(true), Some(false), None, Some(false)]);
    assert_eq!(copying(in_order(link(), Input::Pointer)), [Some(false), Some(true), None, Some(false)]);
}

#[gpui_kit::test]
fn a_copy_button_shown_under_the_pointer_isnt_drawn_under_touch(cx: &mut gpui_kit::TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(crate::theme::apply);

    let shown = |cx: &mut gpui_kit::App| crate::copy_button_shown_while_over("row", "copy", "it", "Copy", cx).is_some();
    assert!(cx.update(shown), "With a pointer, the button shows under it");
    cx.update(|cx| Input::Touch.choose(cx));
    assert!(!cx.update(shown), "Under touch, nothing rests on it, so there's no button");
}
