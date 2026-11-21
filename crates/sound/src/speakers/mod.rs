//! The speakers: playing audio through the system's default output device, and saying when what was given before
//! each mark has gone to it, a moment before it's heard.

mod device;
mod failure;
mod queue;
#[cfg(test)]
mod tests;

pub use device::{Played, Speakers};
pub use failure::SpeakersError;
