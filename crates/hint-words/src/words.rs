//! A field's hint words.

use serde::{Deserialize, Serialize};

use crate::PickedFrom;

/// A word or phrase a field favours.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HintWord {
    pub text: String,
    /// Whether it's written as it's hinted where it's hyphenated and its words are heard spaced or joined, as a
    /// package's name is. The workspace's own names, its branches', repositories' and files', aren't, since a person
    /// says their words in prose too, such as "the stale reports" on a branch `stale-reports`.
    pub spelled: bool,
}

/// A field's hint words, most immediate first, and what they were picked from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Picked {
    pub words: Vec<HintWord>,
    pub from: PickedFrom,
}
