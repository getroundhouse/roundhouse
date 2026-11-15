//! Hearing speech and turning it into text, on this device. No UI and no async runtime: an engine is loaded on a
//! thread of its own, which hears one recording at a time, and each recording's text comes back through a channel
//! that both threads and async tasks can read, as `client-core`'s subscriptions do.
//!
//! [`Dictation`] holds the thread. Each [`Recording`] is heard twice: as it's said, for live text, and a sentence at a
//! time, cut at the pauses where the live text ends one, for each sentence's final text, which can change words the
//! live text had. An [`Engine`] is the model that does both. The desktop app's composer dictates with it, as
//! `apps/desktop ──> dictation` in `docs/architecture.md` has it.

#![warn(clippy::wildcard_enum_match_arm)]

mod audio;
mod dictation;
mod engine;
#[cfg(any(test, feature = "testing"))]
pub mod fake;
pub mod model;
pub mod nemotron;
mod pauses;
mod samples;

pub use audio::{AudioIn, HEARS, Permission, ask_for_microphone, microphone_permission};
pub use dictation::{Dictation, DictationError, Ending, Recording, Update};
pub use engine::{Engine, EngineError, Heard, Listening};
pub use samples::{SAMPLE_RATE, Samples};
