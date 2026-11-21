//! Every colour the app draws with, named by what it's for rather than by its value, so a second theme would be a
//! second set of values for the same names. The app is dark only, so there's one set. Its greys and text are made
//! from a few numbers, as one ramp planned in Oklab, with each text at a contrast on the page, in `palette`. The kit's
//! theme is made from these, in `crate::theme`, so the kit's own parts draw with them too. `docs/spec/look.md` has the
//! table of them, which a test holds to these.

#[cfg(test)]
mod tests;

mod contrast;
mod made;
mod oklab;
mod palette;
mod ramp;
mod srgb;

#[cfg(any(test, feature = "testing"))]
pub use palette::NamedColor;
#[cfg(test)]
use palette::hsla;
pub use palette::{Colors, colors};
