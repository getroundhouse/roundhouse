use gpui_kit::Hsla;

use super::palette::hsla;

/// An opaque colour as the screen is given it, a byte for each of red, green and blue, which every colour the
/// palette makes ends as, so each is exactly a colour the kit can draw and the spec's table can write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Srgb(pub(super) [u8; 3]);

impl Srgb {
    /// From `0xrrggbb`.
    pub(super) const fn from_hex(hex: u32) -> Self {
        let [_, red, green, blue] = hex.to_be_bytes();
        Self([red, green, blue])
    }

    /// As `0xrrggbbaa`, opaque, as the palette writes the colours it doesn't make.
    pub(super) fn rgba(self) -> u32 {
        let [red, green, blue] = self.0;
        u32::from_be_bytes([red, green, blue, 0xff])
    }

    /// This colour over `under` at `strength`, from 0 for none of it to 1 for all of it, mixed a byte at a time, as
    /// paint over the screen would be.
    pub(super) fn over(self, under: Srgb, strength: f64) -> Srgb {
        let mix = |over: u8, under: u8| {
            let (over, under) = (f64::from(over), f64::from(under));
            byte(under + strength * (over - under))
        };
        let ([r, g, b], [ur, ug, ub]) = (self.0, under.0);
        Srgb([mix(r, ur), mix(g, ug), mix(b, ub)])
    }
}

impl From<Srgb> for Hsla {
    fn from(color: Srgb) -> Self {
        hsla(color.rgba())
    }
}

/// The byte nearest `value`, a channel from 0 to 255, with anything outside that range held to it.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "rounded and held between 0 and 255 first, so it's a byte"
)]
pub(super) fn byte(value: f64) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}
