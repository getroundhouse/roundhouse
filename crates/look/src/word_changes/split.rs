//! A run of lines split into the words that are compared: each run of letters, digits and `_`, each run of space, and
//! each other character alone, so a change to `self.opened.clear()` marks only the word that changed.

use std::ops::Range;

/// Past this many words on either side, space and punctuation included, a run is too long to compare, and nothing in
/// it is marked. Comparing a run costs about as much as its two counts of words multiplied, and the transcript makes
/// its marks on the main thread: two unrelated runs of 600 took 1 to 3 ms in a release build on a busy Mac.
const MOST_WORDS: usize = 600;

/// The words of a run of lines, in order.
pub(super) struct Split<'a> {
    words: Vec<&'a str>,
    places: Vec<Place>,
    /// How much the words before each one weigh, then how much all of them do.
    before: Vec<usize>,
    lines: usize,
}

/// Where a word is.
struct Place {
    line: usize,
    /// Its first byte in the line.
    at: usize,
    /// It's the first word of its line other than space.
    starts_line: bool,
}

/// Whether `c` is part of a word, rather than a word of its own or space.
pub(super) fn in_a_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl<'a> Split<'a> {
    /// The words of `lines` after their first `start` bytes, or `None` when there are too many to compare.
    pub(super) fn of(lines: &[&'a str], start: usize) -> Option<Self> {
        let mut split = Self { words: Vec::new(), places: Vec::new(), before: vec![0], lines: lines.len() };
        let mut weight = 0;
        for (line, text) in lines.iter().enumerate() {
            let mut at = start.min(text.len());
            let mut starts_line = true;
            for word in words(&text[at..]) {
                if split.words.len() == MOST_WORDS {
                    return None;
                }
                let space = word.trim().is_empty();
                split.words.push(word);
                split.places.push(Place { line, at, starts_line: starts_line && !space });
                starts_line &= space;
                weight += word.chars().filter(|c| !c.is_whitespace()).count();
                split.before.push(weight);
                at += word.len();
            }
        }
        Some(split)
    }

    pub(super) fn words(&self) -> &[&'a str] {
        &self.words
    }

    pub(super) fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub(super) fn lines(&self) -> usize {
        self.lines
    }

    /// How many characters other than space `words` have.
    pub(super) fn weight(&self, words: &Range<usize>) -> usize {
        self.before[words.end] - self.before[words.start]
    }

    /// The line word `word` is the first of, other than space.
    pub(super) fn line_it_starts(&self, word: usize) -> Option<usize> {
        let place = &self.places[word];
        place.starts_line.then_some(place.line)
    }

    /// Marks `words` on the lines they're on.
    pub(super) fn mark(&self, words: &Range<usize>, lines: &mut [Vec<Range<usize>>]) {
        for (word, place) in self.words[words.clone()].iter().zip(&self.places[words.clone()]) {
            lines[place.line].push(place.at..place.at + word.len());
        }
    }
}

/// The words of `text`, in order.
fn words(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let first = rest.chars().next()?;
        let kind = Kind::of(first);
        let end = match kind {
            Kind::Other => first.len_utf8(),
            Kind::Word | Kind::Space => rest.find(|c| Kind::of(c) != kind).unwrap_or(rest.len()),
        };
        let (word, after) = rest.split_at(end);
        rest = after;
        Some(word)
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Word,
    Space,
    Other,
}

impl Kind {
    fn of(c: char) -> Self {
        if in_a_word(c) {
            Self::Word
        } else if c.is_whitespace() {
            Self::Space
        } else {
            Self::Other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_letters_digits_and_underscores_with_each_other_character_alone() {
        let split: Vec<&str> = words("  self.opened_2.clear(é);").collect();

        assert_eq!(split, ["  ", "self", ".", "opened_2", ".", "clear", "(", "é", ")", ";"]);
    }

    #[test]
    fn a_run_with_too_many_words_isnt_split() {
        let line = "a ".repeat(MOST_WORDS / 2);

        assert!(Split::of(&[&line], 0).is_some());
        assert!(Split::of(&[&line, "b"], 0).is_none());
    }
}
