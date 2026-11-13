//! The list scrolling down to the latest row by itself, over a moment rather than at once, as the button back to it
//! sends it. The latest row can move on the way, as more of the session arrives and as the rows passed turn out
//! other heights than they were taken to be. So rather than going to a place worked out as it set off, each frame
//! moves by a share of what it finds is left, as a `look::motion::Scrolling` says, and the list arrives at the latest
//! row wherever that has got to.

#[cfg(test)]
use super::State;

#[cfg(test)]
mod tests;

mod jumping;
