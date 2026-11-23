//! A recording to hear, what's said in it, and the words each condition hints while it's heard.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Where a clip comes from, which is also what its scores are grouped by.
#[derive(Serialize, Deserialize, ValueEnum, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Set {
    /// Tech-Sentences' test split: one speaker's dictation, dense in developer terms.
    TechSentences,
    /// `LibriSpeech`'s test-clean, read from books by many voices, with Le et al.'s lists of 100 words.
    Librispeech,
    /// Tech-Sentences' training split, where an everyday word is said and a technical one that sounds like it, such
    /// as `crate` for "create", is hinted.
    SoundAlikes,
    /// The same, where what's hinted is the word said with a capital, a product's name, such as `Signal` for "signal".
    Capitals,
}

impl Set {
    pub(crate) const ALL: [Self; 4] = [Self::TechSentences, Self::Librispeech, Self::SoundAlikes, Self::Capitals];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::TechSentences => "tech-sentences",
            Self::Librispeech => "librispeech",
            Self::SoundAlikes => "sound-alikes",
            Self::Capitals => "capitals",
        }
    }

    pub(crate) fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|set| set.name() == name)
    }

    /// Whether what's said in it is written with capitals, as a person would, so a term is spelled right only with
    /// its own. `LibriSpeech`'s is all lower case.
    pub(crate) fn is_cased(self) -> bool {
        !matches!(self, Self::Librispeech)
    }
}

/// Which words a clip is heard with.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Condition {
    NoHints,
    /// Only the terms said, and for a sound-alike, the word it sounds like.
    Said,
    /// Those among unrelated words, as a field gets them.
    AmongOthers,
}

impl Condition {
    /// The order a clip is heard in: first among others, as a field gives words, and then with fewer. The engine
    /// remembers the first hearing's encodings for the others, so only its times are what a person would wait.
    pub(crate) const HEARD: [Self; 3] = [Self::AmongOthers, Self::Said, Self::NoHints];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::NoHints => "no-hints",
            Self::Said => "said",
            Self::AmongOthers => "among-others",
        }
    }
}

/// A recording, and what's said in it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Clip {
    pub(crate) set: Set,
    /// Its dataset's own name for it.
    pub(crate) id: String,
    /// What's said, as it should be written.
    pub(crate) text: String,
    /// The terms in it a person would want spelled right, hinted but for [`Condition::NoHints`].
    pub(crate) terms: Vec<String>,
    /// Words that sound like something said and aren't, hinted with the terms: the sound-alikes'.
    pub(crate) sound_alikes: Vec<String>,
    /// Words that aren't said, hinted only [`Condition::AmongOthers`].
    pub(crate) unrelated: Vec<String>,
    pub(crate) audio: Source,
}

/// Where a clip's audio is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Source {
    /// Bytes of a file at a URL, a FLAC or WAV file of its own.
    Range { url: String, start: u64, length: u64 },
    /// A file in the eval's cache, by its path in it.
    Cached(String),
    /// Samples already at the model's rate, for a test.
    Samples(Vec<f32>),
}

impl Clip {
    /// The words `condition` hints, in order: the terms, then what sounds like them, then the unrelated words.
    pub(crate) fn hint_words(&self, condition: Condition) -> Vec<String> {
        let unrelated: &[String] = if condition == Condition::AmongOthers { &self.unrelated } else { &[] };
        match condition {
            Condition::NoHints => Vec::new(),
            Condition::Said | Condition::AmongOthers => {
                self.terms.iter().chain(&self.sound_alikes).chain(unrelated).cloned().collect()
            }
        }
    }

    /// Every word any condition hints that isn't said, which the score counts when it's written.
    pub(crate) fn unsaid(&self) -> impl Iterator<Item = &str> {
        self.sound_alikes.iter().chain(&self.unrelated).map(String::as_str)
    }

    /// Its name in results: its set's, then its own.
    pub(crate) fn key(&self) -> String {
        format!("{}/{}", self.set.name(), self.id)
    }
}
