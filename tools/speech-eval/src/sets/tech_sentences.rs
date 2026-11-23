//! [Tech-Sentences](https://huggingface.co/datasets/danielrosehill/Tech-Sentences), under CC BY 4.0: one speaker's
//! recordings of dictation-style sentences dense in developer terms, each with the terms in it. Its index says where
//! each clip's file is in the dataset's archives, so each clip is fetched alone.
//!
//! Sentences the dataset took from elsewhere are left out, since its licence doesn't say what theirs are: those it
//! mined, and passages of books.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::Context as _;
use models::ModelFile;
use serde::Deserialize;

use super::{AMONG, spread, unrelated};
use crate::cache::{Cache, Pinned};
use crate::clip::{Clip, Set, Source};

/// The dataset's revision, pinned so every run hears the same clips, since the dataset grows.
macro_rules! revision {
    () => {
        "7ff1bf5e42d0e9c6ca00f318bf2b0474932ad43d"
    };
}

pub(crate) const TECH_SENTENCES: &str = revision!();

/// Where the dataset's files are, at its revision.
const BASE: &str = concat!("https://huggingface.co/datasets/danielrosehill/Tech-Sentences/resolve/", revision!());

const TEST: Pinned = Pinned {
    url: concat!(
        "https://huggingface.co/datasets/danielrosehill/Tech-Sentences/resolve/",
        revision!(),
        "/meta/test.jsonl"
    ),
    file: ModelFile::new(
        concat!("tech-sentences/", revision!(), "/test.jsonl"),
        5_981_224,
        "e9fec716ca8f4328e632d55c98a635077f08f291b5b42d4fa398bbc23ff1a916",
    ),
};

const TRAIN: Pinned = Pinned {
    url: concat!(
        "https://huggingface.co/datasets/danielrosehill/Tech-Sentences/resolve/",
        revision!(),
        "/meta/train.jsonl"
    ),
    file: ModelFile::new(
        concat!("tech-sentences/", revision!(), "/train.jsonl"),
        27_897_165,
        "e57e530ed2478cbbb70944992710677d91632dbf3c4fcea4713c664b581bfc15",
    ),
};

/// How many clips each sound-alike is heard in, at most.
const PER_SOUND_ALIKE: usize = 8;

/// A word said in the training split, and one that sounds like it and isn't said, hinted as though a field had it.
#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SoundAlike {
    pub(crate) said: String,
    pub(crate) hinted: String,
}

impl SoundAlike {
    /// Whether it's the word said with a capital, a product's name, rather than another word.
    pub(crate) fn is_capital(&self) -> bool {
        self.said != self.hinted && self.said.to_lowercase() == self.hinted.to_lowercase()
    }

    /// Whether `text` says it somewhere other than a sentence's start, where it would have a capital anyway, and
    /// doesn't say what's hinted: with that capital, or for another word, in any case.
    fn is_said_in(&self, text: &str) -> bool {
        let hinted_said = if self.is_capital() {
            has_word(text, &self.hinted)
        } else {
            has_word(&text.to_lowercase(), &self.hinted.to_lowercase())
        };
        mid_sentence(text, &self.said) && !hinted_said
    }
}

/// One clip in the index, with what the eval reads of it.
#[derive(Deserialize)]
struct Row {
    id: String,
    text: String,
    target_terms: Vec<String>,
    prompt_source: String,
    /// The archive its file is in, and where.
    shard: String,
    audio_offset: u64,
    audio_length: u64,
}

impl Row {
    /// Whether the dataset's licence covers what's said in it.
    fn is_its_own(&self) -> bool {
        !(self.prompt_source.starts_with("mined:") || self.prompt_source.starts_with("book:"))
    }

    /// Its clip, with its terms but those its sentence doesn't say as the scorer finds them, such as "Z alias" for "a
    /// z alias", which couldn't be scored.
    fn clip(mut self, set: Set, sound_alikes: Vec<String>, pool: &BTreeSet<String>) -> Clip {
        let text = self.text.clone();
        self.target_terms.retain(|term| crate::score::says(&text, term, true));
        let said = self.text.to_lowercase();
        let hinted: BTreeSet<String> =
            self.target_terms.iter().chain(&sound_alikes).map(|w| w.to_lowercase()).collect();
        let others = pool.iter().filter(|term| {
            let term = term.to_lowercase();
            !said.contains(&term) && !hinted.contains(&term)
        });
        let count = AMONG.saturating_sub(self.target_terms.len() + sound_alikes.len());
        let unrelated = unrelated(&self.id, others.map(String::as_str), count);
        let audio = Source::Range {
            url: format!("{BASE}/{}", self.shard),
            start: self.audio_offset,
            length: self.audio_length,
        };
        Clip { set, id: self.id, text: self.text, terms: self.target_terms, sound_alikes, unrelated, audio }
    }
}

/// The test split's clips.
pub(super) fn test(cache: &Cache) -> anyhow::Result<Vec<Clip>> {
    let rows = rows(&cache.file(&TEST)?)?;
    let pool = terms(&rows);
    Ok(rows.into_iter().filter(Row::is_its_own).map(|row| row.clip(Set::TechSentences, Vec::new(), &pool)).collect())
}

/// The training split's clips that say one of `sound_alikes`' words, where it isn't the first word of a sentence
/// and what sounds like it isn't said too, hinted with it: for [`Set::Capitals`] those that differ from what's said
/// only in a capital, and for [`Set::SoundAlikes`] the rest. Their unrelated words are the test split's terms.
pub(super) fn sound_alikes(set: Set, sound_alikes: &[SoundAlike], cache: &Cache) -> anyhow::Result<Vec<Clip>> {
    let pool = terms(&rows(&cache.file(&TEST)?)?);
    let mut train: Vec<Row> = rows(&cache.file(&TRAIN)?)?.into_iter().filter(Row::is_its_own).collect();
    train.sort_by(|a, b| a.id.cmp(&b.id));
    let mut hinted: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for alike in sound_alikes.iter().filter(|alike| alike.is_capital() == (set == Set::Capitals)) {
        let saying: Vec<&Row> = train.iter().filter(|row| alike.is_said_in(&row.text)).collect();
        for row in spread(saying, Some(PER_SOUND_ALIKE)) {
            hinted.entry(row.id.clone()).or_default().push(alike.hinted.clone());
        }
    }
    let clips = train.into_iter().filter_map(|row| {
        let hinted = hinted.remove(&row.id)?;
        Some(row.clip(set, hinted, &pool))
    });
    Ok(clips.collect())
}

fn rows(index: &Path) -> anyhow::Result<Vec<Row>> {
    let text = std::fs::read_to_string(index).with_context(|| format!("couldn't read {}", index.display()))?;
    text.lines()
        .enumerate()
        .map(|(i, line)| serde_json::from_str(line).with_context(|| format!("{}:{}", index.display(), i + 1)))
        .collect()
}

/// Every term the clips say.
fn terms(rows: &[Row]) -> BTreeSet<String> {
    rows.iter().flat_map(|row| row.target_terms.iter().cloned()).collect()
}

/// Whether `word` is in `text` as a word of its own.
fn has_word(text: &str, word: &str) -> bool {
    positions(text, word).next().is_some()
}

/// Whether `word` is in `text` as a word of its own, somewhere other than a sentence's start.
fn mid_sentence(text: &str, word: &str) -> bool {
    positions(text, word).any(|at| {
        let before = text[..at].trim_end();
        !before.is_empty() && !before.ends_with(['.', '?', '!', ':'])
    })
}

fn positions<'t>(text: &'t str, word: &'t str) -> impl Iterator<Item = usize> + 't {
    text.match_indices(word).map(|(at, _)| at).filter(move |&at| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}
