//! Switches: a choice of one of a few, each a segment, with the one chosen on, such as the diff's views or the image
//! comparison's. `docs/spec/components.md` has the table of its kinds, which a test holds to these.

#[cfg(test)]
mod tests;

mod kinds;
mod switch;
pub use kinds::SwitchKind;
pub use switch::{Switch, switch};
