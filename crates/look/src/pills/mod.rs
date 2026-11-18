//! Pills: a value as it was written, such as a command or a search's pattern, filled, in code's type, which opens
//! nothing. `docs/spec/components.md` has the table of its kinds, which a test holds to these.

#[cfg(test)]
mod tests;

mod kinds;
mod pill;
pub use kinds::PillKind;
pub(crate) use pill::HEIGHT;
pub use pill::{Pill, pill};
