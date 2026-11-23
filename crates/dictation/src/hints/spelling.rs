//! Writing a hyphenated hint as it's hinted where the model wrote its parts another way. Hinted, the model still
//! writes many hyphenated names with spaces, as `sentence transformers`, or joined, as `NodeFetch`. Hints spaced too,
//! as `words.rs` gives them, get the parts right more often but write them spaced, and this puts the hyphens back:
//! `tools/speech-eval` found that the two together spell over half again as many hyphenated terms right as the hints
//! alone, where the spaced hints alone spell fewer.

use super::Hints;

impl Hints {
    /// `text` with each hyphenated hint that's [`crate::Hint::spelled`], such as `sqlite-vec`, written as it's hinted wherever its parts are, in any
    /// case, with a hyphen, a space or nothing in place of each of its hyphens and a space or a hyphen in place of each
    /// of its spaces. A hint that starts with a small letter keeps the capital a sentence starts with. The longest hint
    /// that fits wins, and the earliest where two overlap.
    pub(crate) fn spell(&self, text: &str) -> String {
        if self.spelled().is_empty() {
            return text.to_string();
        }
        let mut hyphenated: Vec<&str> = self.spelled().iter().map(String::as_str).collect();
        hyphenated.sort_by_key(|phrase| std::cmp::Reverse(phrase.len()));
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        while at < text.len() {
            let rest = &text[at..];
            let starts_a_word = !text[..at].chars().next_back().is_some_and(joins_a_word);
            let found = starts_a_word.then(|| {
                hyphenated.iter().find_map(|hint| {
                    let length = length_at(rest, hint)?;
                    let ends_a_word = !rest[length..].chars().next().is_some_and(joins_a_word);
                    ends_a_word.then_some((*hint, length))
                })
            });
            if let Some((hint, length)) = found.flatten() {
                out.push_str(&cased(hint, &rest[..length], starts_a_sentence(&text[..at])));
                at += length;
            } else {
                let next = rest.chars().next().map_or(1, char::len_utf8);
                out.push_str(&rest[..next]);
                at += next;
            }
        }
        out
    }
}

/// Whether `c` is part of the word beside it, so a hint isn't found inside a longer name such as `my-sqlite-vec`.
fn joins_a_word(c: char) -> bool {
    c.is_alphanumeric() || c == '-' || c == '_'
}

/// How many bytes of `text`'s start are `hint`, its parts in any case, with each hyphen written as `-`, a space or
/// nothing, and each space as a space or `-`. Its parts hold neither, so where `text` has one, it's the separator.
fn length_at(text: &str, hint: &str) -> Option<usize> {
    let mut chars = text.char_indices().peekable();
    for wanted in hint.chars() {
        match wanted {
            '-' => {
                chars.next_if(|&(_, c)| c == '-' || c == ' ');
            }
            ' ' => {
                chars.next_if(|&(_, c)| c == '-' || c == ' ')?;
            }
            _ => {
                let (_, c) = chars.next()?;
                if !c.to_lowercase().eq(wanted.to_lowercase()) {
                    return None;
                }
            }
        }
    }
    Some(chars.peek().map_or(text.len(), |&(at, _)| at))
}

/// `hint`, with a capital first letter where it has none and what it replaces, `written`, starts a sentence with one.
fn cased(hint: &str, written: &str, sentence_begins: bool) -> String {
    let mut chars = hint.chars();
    match chars.next() {
        Some(first) if sentence_begins && first.is_lowercase() && written.starts_with(char::is_uppercase) => {
            first.to_uppercase().chain(chars).collect()
        }
        _ => hint.to_string(),
    }
}

/// Whether what comes after `before` starts a sentence: nothing but spaces and quotes, or those after a sentence's end.
fn starts_a_sentence(before: &str) -> bool {
    let before = before.trim_end_matches(|c: char| c.is_whitespace() || "\"'(“‘".contains(c));
    before.is_empty() || before.ends_with(['.', '?', '!', ':'])
}
