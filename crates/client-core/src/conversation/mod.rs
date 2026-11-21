//! Conversation mode, on the device that has a chat's conversation: reading the agent's replies out, and telling the
//! hub how far each was heard. What's here is the same for every client. Hearing and speaking are each app's, through
//! the crates that do them, so this does no audio.

mod bar;
mod here;
mod reading;
mod reading_out;
mod telling;

pub use bar::{Bar, HereNow, Shows, Stage, VoiceReady};
pub use here::Conversation;
pub use reading::Reading;
pub use reading_out::{ReadingOut, Voice};
pub use telling::{Do, FromVoice, ToHub, ToVoice, VoiceText};

#[cfg(test)]
mod tests;
