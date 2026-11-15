use gpui_kit::{DefiniteLength, Length, Styled as _, div};

use super::Space;
use crate::spec_table;

#[test]
fn the_specs_table_has_every_step_of_space_in_order_as_the_app_has_it() {
    let table: Vec<[String; 2]> = spec_table::rows("**Every step of space, by its name.**");

    let stepped: Vec<[String; 2]> = Space::ALL.iter().map(|space| space.written()).collect();
    assert_eq!(table, stepped, "docs/spec/look.md's table of spaces, against crates/look/src/space/steps.rs");
}

/// Each step is on the 4px grid and more room than the one before it, so a step's name says how much room it is
/// beside the others.
#[test]
fn every_step_is_on_the_grid_and_more_room_than_the_one_before_it() {
    for &space in Space::ALL {
        assert_eq!(f32::from(space.size()) % 4., 0., "{space:?} is on the 4px grid");
    }
    for pair in Space::ALL.windows(2) {
        assert!(pair[0].size() < pair[1].size(), "{:?} is more room than {:?}", pair[1], pair[0]);
    }
}

/// A gap, a padding and a margin that take a step by its name are as much room as the step says, in pixels, rather than
/// in rems or left to the layout.
#[test]
fn a_gap_a_padding_and_a_margin_are_as_much_room_as_their_step() {
    for &space in Space::ALL {
        let mut spaced = div().gap(space).px(space).mt(space);
        let style = spaced.style();
        let room = Some(DefiniteLength::from(space.size()));

        assert_eq!([style.gap.width, style.gap.height], [room; 2], "{space:?}'s gap");
        assert_eq!([style.padding.left, style.padding.right], [room; 2], "{space:?}'s padding");
        assert_eq!(style.margin.top, Some(Length::from(space.size())), "{space:?}'s margin");
    }
}
