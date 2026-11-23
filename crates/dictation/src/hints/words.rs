//! Cleaning and bounding the hint words a caller supplies for a recording.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

/// The most hint phrases kept for one recording, in the order they were given, so the closest context stays sharp.
const MAX_HINTS: usize = 160;

/// The longest single hint phrase kept, in bytes, so prose sentences or blobs never enter the graph.
const MAX_HINT_BYTES: usize = 48;

/// Words and short phrases a recording is likely to hear, such as the workspace's branch, changed file stems, and
/// identifiers in the chat. Ordered from most immediate to broadest, deduplicated case-insensitively while keeping
/// the first spelling given, and bounded so noise cannot drown out real speech.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hints {
    phrases: Vec<String>,
    /// The hyphenated phrases that are written as they're hinted, as [`Hint::spelled`] says, by how they're kept.
    spelled: Vec<String>,
}

/// A word or phrase to favour, as a field gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hint {
    pub text: String,
    /// Whether, if it's hyphenated, such as a package's name, it's written as it's hinted where the model writes its
    /// words spaced or joined, as `spelling.rs` says. One a person also says as words, such as a branch's name, isn't.
    pub spelled: bool,
}

impl From<&str> for Hint {
    fn from(text: &str) -> Self {
        Self { text: text.to_string(), spelled: true }
    }
}

impl From<String> for Hint {
    fn from(text: String) -> Self {
        Self { text, spelled: true }
    }
}

impl From<&String> for Hint {
    fn from(text: &String) -> Self {
        text.as_str().into()
    }
}

impl Hints {
    /// Cleans, deduplicates and bounds `words`, keeping the earliest spelling of each phrase, which is written as it's
    /// hinted if any of the words it came from is.
    pub fn new(words: impl IntoIterator<Item = impl Into<Hint>>) -> Self {
        let mut kept = HashMap::new();
        let mut hints = Self::default();
        for hint in words {
            let hint = hint.into();
            for candidate in expand(&hint.text) {
                if hints.phrases.len() >= MAX_HINTS {
                    return hints;
                }
                let at = match kept.entry(candidate.to_lowercase()) {
                    Entry::Occupied(entry) => *entry.get(),
                    Entry::Vacant(entry) => {
                        hints.phrases.push(candidate);
                        *entry.insert(hints.phrases.len() - 1)
                    }
                };
                let phrase = &hints.phrases[at];
                if hint.spelled && phrase.contains('-') && !hints.spelled.contains(phrase) {
                    hints.spelled.push(phrase.clone());
                }
            }
        }
        hints
    }

    pub fn is_empty(&self) -> bool {
        self.phrases.is_empty()
    }

    pub fn words(&self) -> &[String] {
        &self.phrases
    }

    pub(super) fn spelled(&self) -> &[String] {
        &self.spelled
    }

    /// Where among these hints is the first that favours writing `text` exactly, as it is or with a capital first
    /// letter. The engine boosts it only if its vocabulary has every character in it. `tools/dictation-eval` asks it
    /// of each term a person says.
    #[cfg(any(test, feature = "testing"))]
    pub fn favouring(&self, text: &str) -> Option<usize> {
        self.phrases.iter().position(|phrase| written_forms(phrase).any(|form| form == text))
    }
}

/// How `phrase` can be written once it's heard: as it is, and with a capital first letter when it starts with a
/// lowercase one, since models start sentences in upper case.
pub(super) fn written_forms(phrase: &str) -> impl Iterator<Item = String> {
    let mut chars = phrase.chars();
    let capitalized = chars
        .next()
        .filter(char::is_ascii_lowercase)
        .map(|first| format!("{}{}", first.to_ascii_uppercase(), chars.as_str()));
    std::iter::once(phrase.to_string()).chain(capitalized)
}

/// Turns one raw hint into the spoken forms worth matching: trimmed, collapsed to single spaces, with `snake_case`
/// split into words since speech vocabularies carry spaces rather than underscores, and a `CamelCase` or hyphenated
/// name followed by its words with spaces between them, as the model often writes them. A hyphenated name heard that
/// way is written as it's hinted, as `Hints::spell` says.
fn expand(raw: &str) -> Vec<String> {
    let trimmed = raw.trim().trim_matches(|c: char| matches!(c, '`' | '"' | '\'' | ',' | ';' | '(' | ')' | '[' | ']'));
    if trimmed.is_empty() || trimmed.len() > MAX_HINT_BYTES {
        return Vec::new();
    }
    let mut out = Vec::new();
    if trimmed.contains('_') {
        let spaced = collapse_spaces(&trimmed.replace('_', " ")).to_lowercase();
        if worth_keeping(&spaced) {
            out.push(spaced);
        }
    } else {
        let clean = collapse_spaces(trimmed);
        if worth_keeping(&clean) {
            let spoken = split_camel(&clean).or_else(|| split_hyphens(&clean));
            out.push(clean);
            if let Some(spaced) = spoken.filter(|s| worth_keeping(s)) {
                out.push(spaced);
            }
        }
    }
    out
}

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Splits `WorkspacePane` into `"workspace pane"` so someone saying the two words gets them, while leaving acronyms
/// such as `SQLite`, `GPUI` and `GitHub` alone when one part would be a single letter.
fn split_camel(word: &str) -> Option<String> {
    if word.contains(' ') || word.contains('-') || word.contains('/') || word.contains('.') {
        return None;
    }
    let mut parts = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = word.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        let boundary = i > 0
            && ch.is_uppercase()
            && (chars[i - 1].is_lowercase() || chars.get(i + 1).is_some_and(|next| next.is_lowercase()));
        if boundary && !current.is_empty() {
            parts.push(std::mem::take(&mut current).to_lowercase());
        }
        current.push(ch);
    }
    if !current.is_empty() {
        parts.push(current.to_lowercase());
    }
    (parts.len() >= 2 && parts.iter().all(|part| part.len() >= 3)).then(|| parts.join(" "))
}

/// Splits `sqlite-vec` into `"sqlite vec"`, keeping each part's case, but not a path, whose hyphens are a file's.
fn split_hyphens(phrase: &str) -> Option<String> {
    let between_words =
        phrase.split('-').all(|part| part.ends_with(char::is_alphanumeric) && part.starts_with(char::is_alphanumeric));
    (phrase.contains('-') && between_words && !phrase.contains('/')).then(|| phrase.replace('-', " "))
}

/// Leaves out single characters, pure numbers, hex hashes, and two-letter lowercase fragments that match ordinary words.
fn worth_keeping(phrase: &str) -> bool {
    if phrase.chars().count() < 2 || phrase.chars().all(|c| c.is_ascii_digit() || !c.is_alphabetic()) {
        return false;
    }
    if phrase.len() == 2 && !phrase.chars().all(|c| c.is_ascii_uppercase()) {
        return false;
    }
    let looks_like_hash = !phrase.contains(' ') && phrase.len() >= 12 && phrase.chars().all(|c| c.is_ascii_hexdigit());
    !looks_like_hash
}
