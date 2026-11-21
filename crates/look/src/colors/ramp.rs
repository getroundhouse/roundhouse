use super::contrast::Contrast;
use super::oklab::Oklab;
use super::srgb::Srgb;

/// How many times [`Ramp::text`] halves what it searches, which finds the lightness far more finely than a byte of
/// colour can show it.
const HALVINGS: u32 = 40;

/// The greys, as one ramp: stops at even steps of Oklab's lightness from the darkest to the lightest, all at one
/// chroma on one hue, so each step looks as far from the next as any other, and each grey is as warm as the rest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Ramp {
    /// The colour whose hue the greys take.
    pub(super) tint: Srgb,
    pub(super) chroma: Chroma,
    pub(super) darkest: Lightness,
    pub(super) lightest: Lightness,
    /// How many stops there are, the darkest and the lightest among them, so at least two.
    pub(super) stops: u8,
}

/// A stop on a [`Ramp`], counted up from its darkest, which is 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Stop(pub(super) u8);

/// Oklab's lightness, from 0 for black to 1 for white.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub(super) struct Lightness(pub(super) f64);

/// How far a colour is from a grey of its lightness, as Oklab measures it.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub(super) struct Chroma(pub(super) f64);

impl Ramp {
    /// The grey at `stop`.
    pub(super) fn grey(&self, stop: Stop) -> Srgb {
        debug_assert!(stop.0 < self.stops, "stop {} of a ramp of {}", stop.0, self.stops);
        let apart = (self.lightest.0 - self.darkest.0) / f64::from(self.stops.max(2) - 1);
        self.at(Lightness(self.darkest.0 + f64::from(stop.0) * apart))
    }

    /// The darkest colour of the ramp's hue and chroma, lighter than `page`, whose contrast on it reaches `contrast`,
    /// which is how text is made. It's past the lightest stop, toward white, as text is.
    pub(super) fn text(&self, contrast: Contrast, page: Srgb) -> Srgb {
        let (mut short, mut reaches) = (Oklab::from(page).l, 1.0);
        for _ in 0..HALVINGS {
            let middle = f64::midpoint(short, reaches);
            if Contrast::of(self.at(Lightness(middle)), page) < contrast {
                short = middle;
            } else {
                reaches = middle;
            }
        }
        self.at(Lightness(reaches))
    }

    /// The colour of the ramp's hue and chroma at `lightness`, whether or not a stop is there.
    fn at(&self, lightness: Lightness) -> Srgb {
        let hue = Oklab::from(self.tint).hue();
        Oklab { l: lightness.0, a: self.chroma.0 * hue.cos(), b: self.chroma.0 * hue.sin() }.srgb()
    }
}
