use gpui_kit::{Pixels, px};

use super::{BarSize, RowSize};
use crate::ControlSize;
use crate::spec_table;

/// The least room above and below a button, so it doesn't touch the edges of what it's set in.
const ROOM: Pixels = px(2.);

#[test]
fn the_specs_table_has_every_bar_size_in_order_as_the_app_has_it() {
    let table: Vec<[String; 3]> = spec_table::rows("**Every size of a bar, by its name.**");

    let sized: Vec<[String; 3]> = BarSize::ALL.iter().map(|size| size.written()).collect();
    assert_eq!(table, sized, "docs/spec/look.md's table of bar sizes, against crates/look/src/heights/bars.rs");
}

#[test]
fn the_specs_table_has_every_row_size_in_order_as_the_app_has_it() {
    let table: Vec<[String; 3]> = spec_table::rows("**Every size of a row, by its name.**");

    let sized: Vec<[String; 3]> = RowSize::ALL.iter().map(|size| size.written()).collect();
    assert_eq!(table, sized, "docs/spec/look.md's table of row sizes, against crates/look/src/heights/rows.rs");
}

/// What a bar and a row hold, as the spec's rules say, with room above and below it, an edge under a bar aside.
#[test]
fn every_bar_and_row_holds_its_buttons_with_room_above_and_below() {
    let held = |height: Pixels, button: ControlSize| height - button.height() >= ROOM * 2.;

    for &size in BarSize::ALL {
        let button = match size {
            BarSize::Small => ControlSize::Small,
            BarSize::Regular => ControlSize::Regular,
        };
        assert!(held(size.height() - crate::EDGE, button), "a {size:?} bar holds a {button:?} button");
    }
    for &size in RowSize::ALL {
        assert!(held(size.height(), ControlSize::Small), "a {size:?} row holds a small button");
    }
}
