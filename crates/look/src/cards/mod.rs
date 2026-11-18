//! Cards. A card holds a few parts about one thing, apart from what's around it, such as a hover card, the update card
//! or a plan to approve, framed by where it sits: over the rest, in place, or in the chat. `docs/spec/components.md`
//! has the tables of a card's kinds and widths, which a test holds to these.

#[cfg(test)]
mod tests;

mod card;
mod kinds;
mod widths;
pub use card::{Card, CardHeading, card, card_heading};
pub use kinds::CardKind;
pub use widths::CardWidth;
