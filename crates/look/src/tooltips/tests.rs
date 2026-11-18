use super::style::{CORNER, WIDEST};
use crate::colors::Colors;
use crate::shape::Corner;

#[test]
fn a_tooltip_is_drawn_as_what_opens_over_the_rest_but_with_a_controls_corners() {
    let style = super::style(&Colors::dark()).style.unwrap();
    assert_eq!(style.max_size.width, Some(WIDEST.into()));
    assert_eq!(CORNER, Corner::Control);
    assert_eq!(style.box_shadow, Some(crate::raised::shadow(&Colors::dark())));
}
