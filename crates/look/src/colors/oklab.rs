use super::srgb::{Srgb, byte};

/// A colour in Oklab, the space the greys are planned in, since its lightness looks even to the eye, so stops the same
/// distance apart in it look as far apart as each other, which sRGB's don't. Lightness `l` is from 0 for black to 1
/// for white, `a` is how red rather than green the colour is, and `b` how yellow rather than blue.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Oklab {
    pub(super) l: f64,
    pub(super) a: f64,
    pub(super) b: f64,
}

impl Oklab {
    /// Its hue, as the angle from `a` toward `b`, in radians.
    pub(super) fn hue(self) -> f64 {
        self.b.atan2(self.a)
    }

    /// The colour `share` of the way from this one to `to`, from 0 for this one to 1 for `to`, so the steps between
    /// two colours look even.
    pub(super) fn toward(self, to: Oklab, share: f64) -> Oklab {
        let step = |from: f64, to: f64| from + share * (to - from);
        Oklab { l: step(self.l, to.l), a: step(self.a, to.a), b: step(self.b, to.b) }
    }

    /// The nearest colour a byte a channel can give, with what's beyond sRGB held to its edge.
    pub(super) fn srgb(self) -> Srgb {
        let Oklab { l, a, b } = self;
        let long = (l + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
        let medium = (l - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
        let short = (l - 0.089_484_177_5 * a - 1.291_485_548 * b).powi(3);
        Srgb([
            encoded(4.076_741_662_1 * long - 3.307_711_591_3 * medium + 0.230_969_929_2 * short),
            encoded(-1.268_438_004_6 * long + 2.609_757_401_1 * medium - 0.341_319_396_5 * short),
            encoded(-0.004_196_086_3 * long - 0.703_418_614_7 * medium + 1.707_614_701 * short),
        ])
    }
}

impl From<Srgb> for Oklab {
    fn from(color: Srgb) -> Self {
        let [r, g, b] = color.0.map(linear);
        let long = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
        let medium = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
        let short = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
        Oklab {
            l: 0.210_454_255_3 * long + 0.793_617_785 * medium - 0.004_072_046_8 * short,
            a: 1.977_998_495_1 * long - 2.428_592_205 * medium + 0.450_593_709_9 * short,
            b: 0.025_904_037_1 * long + 0.782_771_766_2 * medium - 0.808_675_766 * short,
        }
    }
}

/// A channel's byte as the light it stands for, from 0 to 1, undoing sRGB's curve.
fn linear(channel: u8) -> f64 {
    let encoded = f64::from(channel) / 255.0;
    if encoded <= 0.040_45 { encoded / 12.92 } else { ((encoded + 0.055) / 1.055).powf(2.4) }
}

/// Light from 0 to 1 as the byte sRGB's curve gives it, with what's beyond that range held to it.
fn encoded(light: f64) -> u8 {
    let light = light.clamp(0.0, 1.0);
    let encoded = if light <= 0.003_130_8 { 12.92 * light } else { 1.055 * light.powf(1.0 / 2.4) - 0.055 };
    byte(255.0 * encoded)
}
