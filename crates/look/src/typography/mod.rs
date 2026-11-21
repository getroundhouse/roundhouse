//! Every kind of text the app sets, named by what it's for rather than by its size, so a size means the same wherever
//! it's set, and two kinds that are the same size today can change apart. `docs/spec/look.md` has the table of them,
//! which a test holds to these.

#[cfg(test)]
mod tests;

mod code_lines;
mod styles;
pub use code_lines::CodeLines;
pub use styles::{STRONG, Type, Typeset};
pub(crate) use styles::{mono_size, rem};
