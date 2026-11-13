use super::{BETWEEN_ARROWS, gutter};

/// A fold in the middle of a file has an arrow up and an arrow down, side by side in the gutter.
#[test]
fn a_folds_two_arrows_fit_the_gutter_in_either_view_whatever_the_lines_go_up_to() {
    let arrows = look::ControlSize::Small.height() * 2. + BETWEEN_ARROWS;
    for highest in [1, 9, 999, 1_000, 100_000, usize::MAX] {
        for split in [false, true] {
            let (_, gutter) = gutter(highest, split);

            assert!(gutter >= arrows, "{highest} lines, split {split}: {gutter:?} for arrows {arrows:?}");
        }
    }
}
