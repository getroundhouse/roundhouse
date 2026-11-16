//! The thread that holds the engine, and each recording it hears.

mod buffer;
mod capture;
mod hearing;
mod joins;
mod recording;
mod thread;

pub use recording::{DictationError, Ending, Recording, Update};
pub use thread::Dictation;

#[cfg(test)]
mod tests;
