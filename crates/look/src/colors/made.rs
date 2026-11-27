use gpui_kit::Hsla;

use super::contrast::{Contrast, least_reaching};
use super::oklab::Oklab;
use super::palette::hsla;
use super::ramp::{Ramp, Stop};
use super::srgb::Srgb;

/// How one of the [`Colors`](super::Colors) is made, as the palette declares it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Made {
    /// A stop on the ramp of greys.
    Grey(Stop),
    /// Text, at a contrast on the page.
    Text(Contrast),
    /// The brand's colour over the page, at a strength from 0 to 1.
    Wash(f64),
    /// The brand's colour paled toward the text, the least of the way that reaches a contrast on the page, so it's as
    /// readable as it needs to be and keeps as much of the brand as it can.
    Tint(Contrast),
    /// A colour of its own, as `0xrrggbbaa`.
    Fixed(u32),
}

/// What the colours that aren't [`Made::Fixed`] are made from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Recipe {
    pub(super) greys: Ramp,
    /// The pane's stop, which text's contrast is measured on, and a wash is over.
    pub(super) page: Stop,
    pub(super) brand: Srgb,
    /// What to read's contrast on the page, which is text's, and what a tint pales toward.
    pub(super) text: Contrast,
}

/// The colour `made` is, from `recipe`. A colour of its own can be given as its `0xrrggbbaa`, or as an [`Srgb`].
pub(super) fn color(made: impl Into<Made>, recipe: &Recipe) -> Hsla {
    let page = recipe.greys.grey(recipe.page);
    match made.into() {
        Made::Grey(stop) => recipe.greys.grey(stop).into(),
        Made::Text(contrast) => recipe.greys.text(contrast, page).into(),
        Made::Wash(strength) => recipe.brand.over(page, strength).into(),
        Made::Tint(contrast) => recipe.tint(contrast).into(),
        Made::Fixed(rgba) => hsla(rgba),
    }
}

impl Recipe {
    /// The brand's colour paled toward the text, the least of the way that reaches `contrast` on the page.
    pub(super) fn tint(&self, contrast: Contrast) -> Srgb {
        let page = self.greys.grey(self.page);
        let (brand, text) = (Oklab::from(self.brand), Oklab::from(self.greys.text(self.text, page)));
        least_reaching(contrast, page, (0.0, 1.0), |share| brand.toward(text, share).srgb())
    }
}

impl From<u32> for Made {
    fn from(rgba: u32) -> Self {
        Made::Fixed(rgba)
    }
}

impl From<Srgb> for Made {
    fn from(color: Srgb) -> Self {
        Made::Fixed(color.rgba())
    }
}
