//! The files a dictation someone chose to keep is written as, in a folder of its own named by its ID: its audio, and
//! what was hinted, what was written and how it ended. The app writes them, and `speech-eval` reads them.

#![warn(clippy::wildcard_enum_match_arm)]

mod file;
mod wav;

pub use file::{AUDIO, Device, Ended, FILE, Field, How, Kept, Said, Text, VERSION};
pub use wav::{HEADER_BYTES, MAX_DATA_BYTES, header};
