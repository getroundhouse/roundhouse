//! Badges and labels. A badge says what's true of the thing beside it, in a word or two, such as `Default`, tinted in
//! its kind's tone, and a label is a tag someone chose, such as a GitHub label, tinted in the colour they gave it.
//! Neither opens anything. `docs/spec/components.md` has the table of a badge's kinds, which a test holds to these.

#[cfg(test)]
mod tests;

mod badge;
mod kinds;
mod label;
pub use badge::{Badge, badge};
pub use kinds::BadgeKind;
pub use label::{Label, label};
