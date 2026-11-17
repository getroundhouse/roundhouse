//! Toggles: a setting that's on or off, flipped where it's shown, such as Settings' Keep computer awake. Its knob is in
//! the muted text's colour while it's off, so an off toggle still reads as one, as `docs/spec/components.md` says.

#[cfg(test)]
mod tests;

mod toggle;
pub use toggle::{Toggle, toggle};
