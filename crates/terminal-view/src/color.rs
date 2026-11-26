//! A colour both ways between client-core's, which a terminal's cells are drawn in, and GPUI's, which the app's look
//! and the grid's painting are in.
use client_core::drawn::Rgb;
use gpui_kit::{Hsla, rgb};

/// To paint a cell in.
pub(crate) fn to_hsla(color: Rgb) -> Hsla {
    rgb(color.hex()).into()
}

/// To paint a cell's faint text in, which a program asks for with SGR 2: at half its colour, as terminals draw it.
#[expect(
    clippy::disallowed_methods,
    reason = "a terminal's faint text, as terminals draw it, rather than the app's strengths"
)]
pub(crate) fn faint(color: Rgb) -> Hsla {
    to_hsla(color).opacity(0.5)
}

/// One of the app's look's, for the terminal to draw a cell that sets none in, to the nearest of client-core's.
pub(crate) fn to_rgb(color: Hsla) -> Rgb {
    let rgba = color.to_rgb();
    Rgb { r: channel(rgba.r), g: channel(rgba.g), b: channel(rgba.b) }
}

#[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "a colour's channel, from 0 to 1")]
fn channel(value: f32) -> u8 {
    (value.clamp(0., 1.) * 255.).round() as u8
}

#[cfg(test)]
mod tests {
    use client_core::drawn::Rgb;

    use super::to_rgb;

    #[test]
    fn a_colour_keeps_its_channels() {
        assert_eq!(to_rgb(gpui_kit::rgb(0x15_1312).into()), Rgb { r: 0x15, g: 0x13, b: 0x12 });
    }
}
