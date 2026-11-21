//! Dimming a text from one of its bytes on, as what of a reply read out loud wasn't heard is. The kit draws each range
//! of a leaf it's given in the colour given with it, over what colour its marks and links have, so the dimmed part is
//! drawn in the same layout as the rest, and a row doesn't change height as the part moves.
//!
//! The byte is in the text as written, Markdown and all, and a leaf's bytes are in what it shows, without Markdown's
//! marks, which the kit doesn't say how to match up. So the text before the byte is parsed as the whole text is: it
//! ends in the leaf the byte's in, as far into it as the byte. A mark that part leaves open, such as `**` whose end is
//! after the byte, shows there as it's written, and isn't in the whole text's leaf, so it's passed over.

use std::collections::HashMap;
use std::ops::Range;

use super::cache::TextKey;
use super::parse::{Leaf, Parsed, parse_shown};
use crate::images_apart::images_apart;

/// Where a text is dimmed from: the leaf its byte is drawn in, and how far into what that leaf shows, with every leaf
/// after it dimmed whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Dimmed {
    leaf: usize,
    from: usize,
}

impl Dimmed {
    /// The range of what each leaf of `leaves` shows that's dimmed, by the leaf's key, for those with any.
    pub(super) fn ranges(self, leaves: &[Leaf]) -> impl Iterator<Item = (u64, Range<usize>)> {
        let shown = leaves.iter().enumerate().skip(self.leaf).map(move |(leaf, shows)| {
            let len = shows.shows().len();
            let from = if leaf == self.leaf { self.from.min(len) } else { 0 };
            (u64::try_from(leaf).unwrap_or(u64::MAX), from..len)
        });
        shown.filter(|(_, range)| !range.is_empty())
    }
}

/// What the text before the byte a text is dimmed from was parsed as, by the text, so it's parsed once for each place,
/// rather than on every frame the text is drawn: how many leaves it has, and what its last shows.
#[derive(Default)]
pub(super) struct Dims(HashMap<TextKey, Before>);

struct Before {
    text: String,
    leaves: usize,
    last: Option<Leaf>,
}

impl Dims {
    /// Where the text known as `known`, which says `text` and was parsed as `parsed`, is dimmed from, from its byte
    /// `from`, which is taken back to a character's start within it.
    pub(super) fn at(&mut self, known: &TextKey, text: &str, from: usize, parsed: &Parsed) -> Dimmed {
        let before = &text[..text.floor_char_boundary(from)];
        if self.0.get(known).is_none_or(|held| held.text != before) {
            let leaves = parse_shown(images_apart(before).into_owned(), parsed.images, None).leaves;
            let made = Before { text: before.to_string(), leaves: leaves.len(), last: leaves.last().cloned() };
            self.0.insert(known.clone(), made);
        }
        let held = &self.0[known];
        let Some(last) = &held.last else { return Dimmed { leaf: 0, from: 0 } };
        let leaf = held.leaves - 1;
        let from = parsed.leaves.get(leaf).map_or(0, |whole| {
            let whole = whole.shows();
            let reached = matched(last.shows(), whole);
            // A leaf doesn't end in a space, so one the text before the byte ends with was heard too.
            let space = if before.ends_with(char::is_whitespace) { spaces_at(&whole[reached..]) } else { 0 };
            reached + space
        });
        Dimmed { leaf, from }
    }

    /// Lets go of what's held for each text `keep` doesn't keep.
    pub(super) fn keep(&mut self, keep: impl Fn(&TextKey) -> bool) {
        self.0.retain(|known, _| keep(known));
    }
}

/// How far into `whole` the start of it that `part` shows reaches: each character of `part` in turn, where it's the
/// next of `whole`, and passed over where it isn't, as a mark left open is.
fn matched(part: &str, whole: &str) -> usize {
    let mut rest = whole.char_indices().peekable();
    let mut reached = 0;
    for c in part.chars() {
        let Some(&(at, next)) = rest.peek() else { break };
        if next == c {
            rest.next();
            reached = at + next.len_utf8();
        }
    }
    reached
}

/// How many bytes of spaces `text` starts with.
fn spaces_at(text: &str) -> usize {
    text.len() - text.trim_start().len()
}

#[cfg(test)]
mod tests {
    use gpui_kit::ElementId;

    use super::super::parse::{Whose, parse};
    use super::*;

    /// What of each leaf of `text` is dimmed from its byte `from` on, by the leaf's key.
    fn dimmed(text: &str, from: usize) -> Vec<(u64, String)> {
        let parsed = parse(text.to_string(), Whose::TheKits);
        let known = TextKey(ElementId::Name("reply".into()));
        let dimmed = Dims::default().at(&known, text, from, &parsed);
        let shown = dimmed.ranges(&parsed.leaves);
        shown
            .map(|(leaf, range)| (leaf, parsed.leaves[usize::try_from(leaf).unwrap()].shows()[range].to_string()))
            .collect()
    }

    fn leaves(said: &[(u64, &str)]) -> Vec<(u64, String)> {
        said.iter().map(|(leaf, shows)| (*leaf, (*shows).to_string())).collect()
    }

    #[test]
    fn a_text_is_dimmed_from_where_its_byte_is_drawn_without_its_marks() {
        let text = "I'd **keep the badge** and let it shrink, then hide it.\n\nThe name stays.";

        let at_then = text.find("then").unwrap();
        assert_eq!(dimmed(text, at_then), leaves(&[(0, "then hide it."), (1, "The name stays.")]));

        let inside_bold = text.find("the badge").unwrap();
        assert_eq!(
            dimmed(text, inside_bold),
            leaves(&[(0, "the badge and let it shrink, then hide it."), (1, "The name stays.")]),
            "a mark left open before it is passed over"
        );

        assert_eq!(dimmed(text, text.find("The name").unwrap()), leaves(&[(1, "The name stays.")]));
        assert_eq!(
            dimmed(text, 0),
            leaves(&[(0, "I'd keep the badge and let it shrink, then hide it."), (1, "The name stays.")])
        );
    }

    #[test]
    fn a_byte_inside_a_character_is_taken_back_to_its_start() {
        let text = "Café first, then the rest.";

        let inside = text.find('é').unwrap() + 1;

        assert_eq!(dimmed(text, inside), leaves(&[(0, "é first, then the rest.")]));
    }

    #[test]
    fn the_text_before_the_byte_is_parsed_once_for_each_place() {
        let text = "Heard so far, and not yet.";
        let parsed = parse(text.to_string(), Whose::TheKits);
        let known = TextKey(ElementId::Name("reply".into()));
        let mut dims = Dims::default();
        assert_eq!(dims.at(&known, text, 6, &parsed), Dimmed { leaf: 0, from: 6 });

        dims.0.get_mut(&known).unwrap().last = Some(Leaf::text("Heard so".to_string()));

        assert_eq!(dims.at(&known, text, 6, &parsed), Dimmed { leaf: 0, from: 9 }, "what it held is what it goes by");
        assert_eq!(dims.at(&known, text, 3, &parsed), Dimmed { leaf: 0, from: 3 }, "until the place moves");
    }
}
