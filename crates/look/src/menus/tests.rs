use super::style::{AROUND, CORNER, ROW, ROW_CORNER};
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
