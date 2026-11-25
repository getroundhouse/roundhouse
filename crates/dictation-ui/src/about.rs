//! What a host says of what's dictated into its field, as each recording starts.

use dictation::{Hint, Hints};
use hint_words::{Picked, PickedFrom};

/// What's dictated is about, to the field's host. Nothing here reads the IDs or what the words were picked from, which
/// go to whatever keeps the recording.
#[derive(Clone, Debug, Default)]
pub struct About {
    /// The words to favour.
    pub hints: Hints,
    /// What they were picked from.
    pub picked_from: PickedFrom,
    /// The workspace it's in, by its ID, unless it's for one not made yet.
    pub workspace: Option<String>,
    /// The chat it's in, by its ID, if it's in one.
    pub chat: Option<String>,
    /// Whether it's said in conversation mode.
    pub said_out_loud: bool,
}

impl From<Picked> for About {
    fn from(Picked { words, from }: Picked) -> Self {
        let hints = Hints::new(words.into_iter().map(|word| Hint { text: word.text, spelled: word.spelled }));
        Self { hints, picked_from: from, ..Self::default() }
    }
}
