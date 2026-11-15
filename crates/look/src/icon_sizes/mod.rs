//! Every size an icon comes in, and where each goes. They're sizes, ordered from the smallest, and each is chosen by
//! where an icon is drawn, so the icons in the same kind of place are the same size, wherever that is.
//! `docs/spec/look.md` has the table of them, which a test holds to these.
//!
//! Each is in pixels, as a corner and a space are, so an icon stays as it is whatever size text is set at.

#[cfg(test)]
mod tests;

mod sizing;
pub use sizing::{IconSize, IconSizing};
