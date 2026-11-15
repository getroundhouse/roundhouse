//! The agent's own failed requests in a row, the same failure each time, as the hub says of each as it records it,
//! drawn as one card: the newest hosts it, with a line for what started each try, and the rows before it that the card
//! says draw nothing.

mod reading;
mod tries;

pub(in crate::rows) use reading::Failures;
pub(crate) use reading::redrawn_with;
#[cfg(test)]
use tries::SHOWN_TRIES;
pub(in crate::rows) use tries::{Started, Tries, Try};

#[cfg(test)]
mod tests;
