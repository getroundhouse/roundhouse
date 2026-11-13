use gpui_kit::{AbsoluteLength, Styled as _, div};

use super::{Corner, Shaped as _};
use crate::spec_table;

#[test]
fn the_specs_table_has_every_corner_in_order_as_the_app_rounds_it() {
    let table: Vec<[String; 2]> = spec_table::rows("**Every corner, by its name.**");

    let rounded: Vec<[String; 2]> = Corner::ALL.iter().map(|corner| corner.written()).collect();
    assert_eq!(table, rounded, "docs/spec/look.md's table of the corners, against crates/look/src/shape.rs");
}

#[test]
fn each_corner_rounds_all_four_as_far_as_it_says() {
    for &corner in Corner::ALL {
        let mut rounded = div().corners(corner);
        let radii = &rounded.style().corner_radii;

        let radius = Some(AbsoluteLength::from(corner.radius()));
        let all = [radii.top_left, radii.top_right, radii.bottom_right, radii.bottom_left];
        assert_eq!(all, [radius; 4], "{corner:?}");
    }
}

#[test]
fn a_corner_on_top_rounds_only_the_top_two() {
    for &corner in Corner::ALL {
        let mut rounded = div().corners_top(corner);
        let radii = &rounded.style().corner_radii;

        let radius = Some(AbsoluteLength::from(corner.radius()));
        let all = [radii.top_left, radii.top_right, radii.bottom_right, radii.bottom_left];
        assert_eq!(all, [radius, radius, None, None], "{corner:?}");
    }
}
