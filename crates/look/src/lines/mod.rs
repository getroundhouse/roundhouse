//! Lines. A line separates what's on either side of it: across what it's in, such as between a card's parts or along
//! the chat with what happened there in its middle, or on one side of what it's set on, such as under a bar, between
//! panes or under a row of Settings. `docs/spec/components.md` has the rules.

#[cfg(test)]
mod tests;

mod line;
mod lined;
pub use line::{Line, line};
pub use lined::Lined;
