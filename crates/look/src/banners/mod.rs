//! Banners. A banner runs across a pane, or the window's top, and says something about all of it, such as an error,
//! setup that failed, or that only some files are listed, with what to do about it at its end.
//! `docs/spec/components.md` has the table of a banner's kinds, which a test holds to these.

#[cfg(test)]
mod tests;

mod banner;
mod kinds;
pub use banner::{Banner, banner};
pub use kinds::BannerKind;
