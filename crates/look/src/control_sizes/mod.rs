//! A control's sizes: every size a control comes in, such as a button or a field, and where each goes, so a field is as
//! tall as the buttons beside it. `docs/spec/look.md` has the table of them, which a test holds to these.

#[cfg(test)]
mod tests;

mod sizes;
pub use sizes::ControlSize;
