use super::top_bar::LINE;
use crate::heights::BarSize;
use crate::traits::Reach;
use crate::typography::{Type, rem};

/// The bar is one size, so its title and the line under it have to fit it, which they wouldn't at a line of body
/// text's and a detail's usual heights.
#[test]
fn a_title_and_the_line_under_it_fit_the_bar_on_a_handheld() {
    let reach = Reach::Handheld;
    let size = |kind: Type| f32::from(kind.size_at(reach, rem(reach)));

    let lines = (size(Type::Body) + size(Type::Detail)) * LINE;

    assert!(lines <= f32::from(BarSize::Regular.height_at(reach)), "{lines}");
}
