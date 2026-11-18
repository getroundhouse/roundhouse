//! Bars. A bar runs along the top of a pane, a sidebar or a card, saying what's under it and holding what's done about
//! it, or holding a row of tabs. `docs/spec/components.md` has the table of a bar's kinds, which a test holds to these.

#[cfg(test)]
mod tests;

mod bar;
mod kinds;
pub use bar::{Bar, bar};
pub use kinds::BarKind;
