//! What a host says of what's dictated into its field, as each recording starts.

use dictation::Hints;

/// What's dictated is about, to the field's host. Nothing here reads the IDs, which go to whatever keeps the recording.
#[derive(Clone, Debug, Default)]
pub struct About {
    /// The words to favour.
    pub hints: Hints,
    /// The workspace it's in, by its ID, unless it's for one not made yet.
    pub workspace: Option<String>,
    /// The chat it's in, by its ID, if it's in one.
    pub chat: Option<String>,
    /// Whether it's said in conversation mode.
    pub said_out_loud: bool,
}
