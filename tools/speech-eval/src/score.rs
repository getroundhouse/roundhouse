//! What's written of a clip, scored against what was said: the terms spelled right, the hinted words written that
//! weren't said, and the errors in everything else, as raw counts so they add up across clips.
//!
//! Terms and hinted words are found in the text as written, so a term is right only spelled as it is, with its own
//! capitals where the set has them. Only at a sentence's start can its first letter be a capital it doesn't have.
//! Everything else is compared as Le et al.'s scorer compares it, after making both lower case and leaving out marks:
//! aligned word by word, each word said that isn't in a term is the same, swapped for another or left out, and a word
//! written that's in no term or hinted word is put in. Their U-WER is those errors over those words.

use serde::{Deserialize, Serialize};

use crate::clip::Clip;

/// One text's counts, or many added up.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Counts {
    /// How many times terms are said.
    pub(crate) terms: usize,
    /// How many of those are written as they should be.
    pub(crate) right: usize,
    /// How many times a hinted word that isn't said is written.
    pub(crate) unsaid: usize,
    /// The words said outside the terms.
    pub(crate) words: usize,
    /// Of those, how many were written as another word.
    pub(crate) swapped: usize,
    /// How many were left out.
    pub(crate) left_out: usize,
    /// The words written that weren't said and are in no term or hinted word.
    pub(crate) put_in: usize,
}

impl Counts {
    pub(crate) fn add(&mut self, other: &Self) {
        self.terms += other.terms;
        self.right += other.right;
        self.unsaid += other.unsaid;
        self.words += other.words;
        self.swapped += other.swapped;
        self.left_out += other.left_out;
        self.put_in += other.put_in;
    }

    /// The errors outside the terms.
    pub(crate) fn errors(&self) -> usize {
        self.swapped + self.left_out + self.put_in
    }
}

/// The version of the scoring here, which a baseline records, so `compare` can say when a baseline's counts are from
/// another. It goes up with each change to how a text is counted.
pub(crate) const SCORER: u32 = 2;

/// `written`'s counts, for what's said in `clip`.
pub(crate) fn score(clip: &Clip, written: &str) -> Counts {
    let cased = clip.set.is_cased();
    let (said, written) = (apostrophes(&clip.text), apostrophes(written));
    let terms = distinct(clip.terms.iter().map(String::as_str));
    let (said_terms, written_terms) = (in_terms(&said, &terms, cased), in_terms(&written, &terms, cased));
    let mut counts = Counts::default();
    for i in 0..terms.len() {
        let times = |found: &[(usize, Span)]| found.iter().filter(|(term, _)| *term == i).count();
        counts.terms += times(&said_terms);
        counts.right += times(&said_terms).min(times(&written_terms));
    }
    let mut hinted: Vec<Span> = written_terms.into_iter().map(|(_, span)| span).collect();
    for word in distinct(clip.unsaid()) {
        let found = unsaid(&written, &said, word, cased);
        counts.unsaid += found.len().saturating_sub(unsaid(&said, &said, word, cased).len());
        hinted.extend(found);
    }
    let said_terms: Vec<Span> = said_terms.into_iter().map(|(_, span)| span).collect();
    let (said_words, written_words) = (words(&said), words(&written));
    for step in align(&said_words, &written_words) {
        match step {
            Step::Same(s, _) | Step::Swapped(s, _) | Step::LeftOut(s) if within(&said_words[s].span, &said_terms) => {}
            Step::Same(..) => counts.words += 1,
            Step::Swapped(..) => (counts.words, counts.swapped) = (counts.words + 1, counts.swapped + 1),
            Step::LeftOut(_) => (counts.words, counts.left_out) = (counts.words + 1, counts.left_out + 1),
            Step::PutIn(w) if !within(&written_words[w].span, &hinted) => counts.put_in += 1,
            Step::PutIn(_) => {}
        }
    }
    counts
}

/// Whether `text` says `phrase` as `score` finds it, as a set that's `cased` has it.
pub(crate) fn says(text: &str, phrase: &str, cased: bool) -> bool {
    !occurrences(&apostrophes(text), &apostrophes(phrase), cased).is_empty()
}

/// How `written` differs from `said`, word by word, with both made lower case and marks left out: `[said→written]`
/// for a word swapped, `[-said]` for one left out and `[+written]` for one put in.
pub(crate) fn diff(said: &str, written: &str) -> String {
    let (said_words, written_words) = (words(&apostrophes(said)), words(&apostrophes(written)));
    let parts: Vec<String> = align(&said_words, &written_words)
        .into_iter()
        .map(|step| match step {
            Step::Same(s, _) => said_words[s].text.clone(),
            Step::Swapped(s, w) => format!("[{}→{}]", said_words[s].text, written_words[w].text),
            Step::LeftOut(s) => format!("[-{}]", said_words[s].text),
            Step::PutIn(w) => format!("[+{}]", written_words[w].text),
        })
        .collect();
    parts.join(" ")
}

/// Where something is in a text, in bytes.
pub(crate) type Span = std::ops::Range<usize>;

fn distinct<'a>(words: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let mut seen: Vec<&str> = words.collect();
    seen.sort_unstable();
    seen.dedup();
    seen
}

/// Where each of `terms` is in `text`, by its place in `terms`. A term inside a longer one found there, as `React` is
/// in `React Native`, isn't found again, so a term said once counts once.
fn in_terms(text: &str, terms: &[&str], cased: bool) -> Vec<(usize, Span)> {
    let mut found: Vec<(usize, Span)> = terms
        .iter()
        .enumerate()
        .flat_map(|(i, term)| occurrences(text, term, cased).into_iter().map(move |at| (i, at)))
        .collect();
    found.sort_by_key(|(_, span)| std::cmp::Reverse(span.len()));
    let mut kept: Vec<(usize, Span)> = Vec::new();
    for (term, span) in found {
        if !kept.iter().any(|(_, longer)| longer.start <= span.start && span.end <= longer.end) {
            kept.push((term, span));
        }
    }
    kept
}

/// Where `phrase` is in `text` as words of its own. Where `cased`, it has to have its own capitals, but at a
/// sentence's start its first letter can be a capital; elsewhere, case doesn't count.
fn occurrences(text: &str, phrase: &str, cased: bool) -> Vec<Span> {
    if !cased {
        return found(text, phrase, false, |_| true);
    }
    let mut chars = phrase.chars();
    let Some(first) = chars.next() else { return Vec::new() };
    let capitalised: String = first.to_uppercase().chain(chars).collect();
    let mut spans = found(text, phrase, true, |_| true);
    if capitalised != phrase {
        spans.extend(found(text, &capitalised, true, |at| starts_a_sentence(text, at)));
    }
    spans
}

/// Where `word`, which isn't said, is written in `text`. At a sentence's start, where any word has a capital, a word
/// that's only a capital away from one said in `said`, as `Photos` is from "photos", isn't counted, since it can't be
/// told from that word.
fn unsaid(text: &str, said: &str, word: &str, cased: bool) -> Vec<Span> {
    if !cased {
        return found(text, word, false, |_| true);
    }
    let mut chars = word.chars();
    let lowered: String = chars.next().into_iter().flat_map(char::to_lowercase).chain(chars).collect();
    let alike = lowered != word && !found(said, &lowered, true, |_| true).is_empty();
    found(text, word, true, |at| !alike || !starts_a_sentence(text, at))
}

/// Where `phrase` is in `text` with no letter or digit either side, with its own case where `cased`, and where `counts`
/// takes where it begins.
pub(crate) fn found(text: &str, phrase: &str, cased: bool, counts: impl Fn(usize) -> bool) -> Vec<Span> {
    if phrase.is_empty() {
        return Vec::new();
    }
    text.char_indices()
        .filter_map(|(at, _)| Some(at..at + length_at(&text[at..], phrase, cased)?))
        .filter(|span| {
            let before = text[..span.start].chars().next_back();
            let after = text[span.end..].chars().next();
            !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char) && counts(span.start)
        })
        .collect()
}

/// How many bytes of `text`'s start are `phrase`, with its own case where `cased`, or in any case.
fn length_at(text: &str, phrase: &str, cased: bool) -> Option<usize> {
    let mut chars = text.char_indices();
    for wanted in phrase.chars() {
        let (_, c) = chars.next()?;
        let same = if cased { c == wanted } else { c.to_lowercase().eq(wanted.to_lowercase()) };
        if !same {
            return None;
        }
    }
    Some(chars.next().map_or(text.len(), |(at, _)| at))
}

/// Whether `at` in `text` starts a sentence: nothing but spaces and quotes before it, or those after a sentence's
/// end.
fn starts_a_sentence(text: &str, at: usize) -> bool {
    let before = text[..at].trim_end_matches(|c: char| c.is_whitespace() || "\"'(“‘".contains(c));
    before.is_empty() || before.ends_with(['.', '?', '!', ':'])
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

/// `text` with curly apostrophes made straight, since the model writes either.
pub(crate) fn apostrophes(text: &str) -> String {
    text.replace('’', "'")
}

/// A word of a text, made lower case, and where it is.
pub(crate) struct Word {
    pub(crate) text: String,
    pub(crate) span: Span,
}

/// `text`'s words, with marks left out but for apostrophes inside a word.
pub(crate) fn words(text: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let mut start = None;
    for (at, c) in text.char_indices().chain([(text.len(), ' ')]) {
        match (start, c.is_alphanumeric() || c == '\'') {
            (None, true) => start = Some(at),
            (Some(from), false) => {
                let word = &text[from..at];
                let trimmed = word.trim_start_matches('\'');
                let from = from + (word.len() - trimmed.len());
                let trimmed = trimmed.trim_end_matches('\'');
                if !trimmed.is_empty() {
                    words.push(Word { text: trimmed.to_lowercase(), span: from..from + trimmed.len() });
                }
                start = None;
            }
            (None, false) | (Some(_), true) => {}
        }
    }
    words
}

/// Whether `span` is inside one of `spans`.
fn within(span: &Span, spans: &[Span]) -> bool {
    spans.iter().any(|outer| outer.start <= span.start && span.end <= outer.end)
}

/// One step of an alignment, by where the words are in what was said and what was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Same(usize, usize),
    Swapped(usize, usize),
    LeftOut(usize),
    PutIn(usize),
}

/// The cheapest way from `said` to `written`, with Le et al.'s costs: 4 for a word swapped, and 3 for one left out
/// or put in, so a swap is cheaper than both. Ties go to a swap, then to putting in, as theirs do.
fn align(said: &[Word], written: &[Word]) -> Vec<Step> {
    let columns = written.len() + 1;
    let mut cost = vec![0u32; (said.len() + 1) * columns];
    let at = |s: usize, w: usize| s * columns + w;
    for s in 0..=said.len() {
        for w in 0..=written.len() {
            cost[at(s, w)] = match (s, w) {
                (0, 0) => 0,
                (0, _) => cost[at(0, w - 1)] + 3,
                (_, 0) => cost[at(s - 1, 0)] + 3,
                _ => {
                    let diagonal = cost[at(s - 1, w - 1)] + if said[s - 1].text == written[w - 1].text { 0 } else { 4 };
                    diagonal.min(cost[at(s, w - 1)] + 3).min(cost[at(s - 1, w)] + 3)
                }
            };
        }
    }
    let mut steps = Vec::new();
    let (mut s, mut w) = (said.len(), written.len());
    while s > 0 || w > 0 {
        let here = cost[at(s, w)];
        if s > 0 && w > 0 {
            let same = said[s - 1].text == written[w - 1].text;
            if here == cost[at(s - 1, w - 1)] + if same { 0 } else { 4 } {
                steps.push(if same { Step::Same(s - 1, w - 1) } else { Step::Swapped(s - 1, w - 1) });
                (s, w) = (s - 1, w - 1);
                continue;
            }
        }
        if w > 0 && here == cost[at(s, w - 1)] + 3 {
            steps.push(Step::PutIn(w - 1));
            w -= 1;
        } else {
            steps.push(Step::LeftOut(s - 1));
            s -= 1;
        }
    }
    steps.reverse();
    steps
}
