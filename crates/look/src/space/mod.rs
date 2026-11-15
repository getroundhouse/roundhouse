//! Every step of space: the room left between and around things, as a gap, a padding or a margin. Each is in pixels, as
//! a corner is, so a space stays as it is whatever size text is set at. `docs/spec/look.md` has the table of them,
//! which a test holds to these.

#[cfg(test)]
mod tests;

mod steps;
pub use steps::Space;
