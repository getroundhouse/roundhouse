use gpui_kit::{AbsoluteLength, Styled as _, div};

use super::{Corner, Shaped as _};

/// The spec's file that has the table of the corners.
const LOOK: &str = include_str!("../../../../docs/spec/look.md");

#[test]
fn the_specs_table_has_every_corner_in_order_as_the_app_rounds_it() {
    let section =
        LOOK.split("\n## Space and shape\n").nth(1).and_then(|rest| rest.split("\n## ").next()).unwrap_or_default();
    let table: Vec<[String; 2]> = section
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .map(|row| {
            let mut cells = row.split(" | ").map(str::to_string);
            let mut cell = || cells.next().unwrap_or_default();
            [cell().trim_end_matches('`').to_string(), cell()]
        })
        .collect();

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
