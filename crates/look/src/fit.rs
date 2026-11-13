//! How an image is fitted to the room there is, wherever it's drawn whole, such as the file tab and a prompt's image
//! opened over the window: both ways, at its own shape, inside a thin edge, and never bigger than it is.

use gpui_kit::{Pixels, Size};

use crate::shape::EDGE;

/// The scale that fits something `width` by `height` pixels, inside its edge, into `room`, both ways, at most its own
/// size, so images drawn at the same scale keep the sizes they have beside each other.
pub fn fit((width, height): (u32, u32), room: Size<Pixels>) -> f32 {
    let across = (f32::from(room.width) - 2. * f32::from(EDGE)) / pixels(width.max(1));
    let down = (f32::from(room.height) - 2. * f32::from(EDGE)) / pixels(height.max(1));
    across.min(down).clamp(0., 1.)
}

/// An image's side in pixels, to draw with.
#[expect(clippy::cast_precision_loss, reason = "an image's side is far below where an f32 loses whole pixels")]
pub fn pixels(side: u32) -> f32 {
    side as f32
}

#[cfg(test)]
mod tests {
    use gpui_kit::{px, size};

    use super::fit;

    #[test]
    fn an_image_is_fitted_both_ways_and_never_drawn_bigger_than_it_is() {
        let room = size(px(802.), px(602.));

        assert!((fit((1600, 900), room) - 0.5).abs() < 1e-6, "by its width");
        assert!((fit((400, 1200), room) - 0.5).abs() < 1e-6, "by its height");
        assert!((fit((200, 100), room) - 1.).abs() < 1e-6, "at its own size");
        assert!(fit((200, 100), size(px(0.), px(0.))).abs() < 1e-6, "with no room");
    }
}
