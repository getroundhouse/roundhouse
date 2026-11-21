//! Hearing speech and turning it into text, on this device. No UI and no async runtime: an engine is loaded on a
//! thread of its own, which hears one recording at a time, and each recording's text comes back through a channel
//! that both threads and async tasks can read, as `client-core`'s subscriptions do. It builds none of the workspace's
//! other crates but `models`, `now-playing` and what they build, so any client on any device can hear with it, as
//! `docs/principles/layers.md` says.
//!
//! [`Dictation`] holds the thread. Each [`Recording`] is heard twice: as it's said, for live text, and a sentence at a
//! time, cut at the pauses where the live text ends one, for each sentence's final text, which can change words the
//! live text had. An [`Engine`] is the model that does both. The desktop app's composer dictates with it.
//!
//! A recording's audio and its loudness are held in memory, and only while it's heard. Nothing here writes either to
//! disk or sends it off the device, because `docs/spec/chats.md` says that what's said never leaves the device.

#![warn(clippy::wildcard_enum_match_arm)]

mod audio_in;
mod dictation;
mod engine;
#[cfg(any(test, feature = "testing"))]
pub mod fake;
mod hints;
mod language;
mod level;
pub mod nemotron;
mod pauses;
mod samples;

pub use audio_in::AudioIn;
pub use dictation::{Dictation, DictationError, Ending, Recording, Update};
pub use engine::{Engine, EngineError, Heard, Listening};
pub use hints::Hints;
pub use language::Language;
pub use level::Level;
pub use samples::{SAMPLE_RATE, Samples};
