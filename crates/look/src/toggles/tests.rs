use gpui_kit::{CursorStyle, Styled as _};

use super::toggle;

/// The hand is the toggle's own, so one that can be flipped has it, and one that's off keeps the arrow.
#[test]
fn a_toggle_has_the_hand_only_while_it_can_be_flipped() {
    let cursor = |off: bool| toggle("setting", true).off(off).drawn().style().mouse_cursor;

    assert_eq!(cursor(false), Some(CursorStyle::PointingHand));
    assert_ne!(cursor(true), Some(CursorStyle::PointingHand));
}
