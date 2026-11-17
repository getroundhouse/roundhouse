//! Fields: a line of text to type in, such as a workspace's new name, a token or a search, drawn as a box of its own
//! or bare in what frames it. `docs/spec/components.md` has the table of its kinds, which a test holds to these.

#[cfg(test)]
mod tests;

mod field;
mod kinds;
pub use field::{Field, field};
pub use kinds::FieldKind;
