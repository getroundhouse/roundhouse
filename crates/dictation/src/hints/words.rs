//! Cleaning and bounding the hint words a caller supplies for a recording.

use std::collections::HashSet;

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
}

impl Hints {
    /// Cleans, deduplicates and bounds `words`, keeping the earliest spelling of each phrase.
    pub fn new(words: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        let mut seen = HashSet::new();
        let mut phrases = Vec::new();
        for raw in words {
            for candidate in expand(raw.as_ref()) {
                if phrases.len() >= MAX_HINTS {
                    return Self { phrases };
                }
                let folded = candidate.to_lowercase();
                if seen.insert(folded) {
                    phrases.push(candidate);
                }
            }
        }
        Self { phrases }
    }

    pub fn is_empty(&self) -> bool {
        self.phrases.is_empty()
    }

    pub fn words(&self) -> &[String] {
        &self.phrases
    }
}

/// Turns one raw hint into the spoken forms worth matching: trimmed, collapsed to single spaces, and with `snake_case`
/// split into words since speech vocabularies carry spaces rather than underscores.
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
            let camel = split_camel(&clean);
            out.push(clean);
            if let Some(spaced) = camel.filter(|s| worth_keeping(s)) {
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
