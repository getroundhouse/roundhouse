//! Dictation in the composer: holding a key or pressing the microphone records what the person says, which shows
//! dimmed under what's typed as it's heard, and goes into the draft at the cursor once each sentence of it is final, as
//! `docs/spec/chats.md` says. `dictation` hears it. Here is what the app holds of it: the person's choice, how it
//! stands as it follows that choice, from downloading and preparing the model to ready, and Space held to dictate.
//! Each recording is its composer's, in `workspace_pane/dictating.rs`.

mod choice;
mod deleting;
mod downloading;
mod microphone;
mod readiness;
mod words;

pub(crate) use choice::DictationChoice;
pub(crate) use downloading::observe_progress;
pub(crate) use microphone::microphone;
pub use microphone::{HIDE, MICROPHONE, TURN_ON};
pub(crate) use readiness::{hears_the_microphone, observe, ready, record, runs_here, start};
#[cfg(feature = "testing")]
pub use readiness::{is_ready, use_engine, use_model};
#[cfg(test)]
pub(crate) use words::gigabytes;
pub(crate) use words::status_box;
pub use words::{DELETE, RETRY, STATUS};
