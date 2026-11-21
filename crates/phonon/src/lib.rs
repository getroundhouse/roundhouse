//! Gradium's Phonon 31m, a model that turns English text into speech on the device's CPU, through the `ptts` crate,
//! as `docs/decisions/phonon-speaks-replies.md` says. It streams, so the first audio of a phrase comes within tens of
//! milliseconds, long before the rest is made. It knows nothing of what it's given to say or where the audio goes, so
//! it doesn't depend on `sound`. No UI and no async runtime, as `docs/principles/layers.md` says.

#![warn(clippy::wildcard_enum_match_arm)]

mod engine;
mod files;
mod speed;

pub use engine::{Phonon, PhononError, Speech};
pub use files::MODEL;
pub use speed::Speed;
