//! A hover card, timed the same as every other in the app and its transcript, which something opened from it can hold
//! open.

mod card;
#[cfg(test)]
mod tests;

pub use card::{HoldOpen, HoverCard, hover_card, hover_card_with_a_button};
