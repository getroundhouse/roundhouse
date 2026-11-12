//! Places in the texts a selection is made in: where a point in the window falls among the texts drawn, what's
//! selected of a text between two places, as a copy has it, and the word around a place.

use std::ops::Range;
use std::sync::Arc;

use gpui_kit::{Pixels, Point, point, px};

use super::{Drawn, Spot};
use crate::prose::{ATOM, TextAt};

/// What's selected from `start` to `end` of `texts`, as text: what's selected of each leaf a line apart, and of each
/// text a blank line apart, leaving out a leaf, or a text, that has none of it selected.
pub(super) fn copied(start: Spot, end: Spot, texts: &[(TextAt, Arc<[String]>)]) -> String {
    let mut copied = Vec::new();
    for (at, leaves) in texts {
        let leaves: Vec<String> = selected(start, end, *at, leaves)
            .filter(|(_, range)| !range.is_empty())
            .filter_map(|(leaf, range)| leaf_text(leaves, leaf)?.get(range))
            .map(|text| text.replace(ATOM, ""))
            .collect();
        if !leaves.is_empty() {
            copied.push(leaves.join("\n"));
        }
    }
    copied.join("\n\n")
}

/// What the leaf `leaf` of `leaves` shows.
pub(super) fn leaf_text(leaves: &[String], leaf: u64) -> Option<&str> {
    leaves.get(usize::try_from(leaf).ok()?).map(String::as_str)
}

/// What's selected from `start` to `end` of the text at `at`, whose leaves are `leaves`, by leaf.
pub(super) fn selected(
    start: Spot,
    end: Spot,
    at: TextAt,
    leaves: &[String],
) -> impl Iterator<Item = (u64, Range<usize>)> + '_ {
    let within = start.at <= at && at <= end.at;
    (0u64..).zip(leaves).filter(move |_| within).filter_map(move |(leaf, text)| {
        let from = if at == start.at { Some((start.leaf, start.byte)) } else { None };
        let to = if at == end.at { Some((end.leaf, end.byte)) } else { None };
        let low = match from {
            Some((first, _)) if leaf < first => return None,
            Some((first, byte)) if leaf == first => byte,
            Some(_) | None => 0,
        };
        let high = match to {
            Some((last, _)) if leaf > last => return None,
            Some((last, byte)) if leaf == last => byte,
            Some(_) | None => text.len(),
        };
        Some((leaf, low.min(text.len())..high.min(text.len())))
    })
}

/// The place in what shows of the texts drawn in this frame nearest `position`: in the leaf under it, or across from
/// it, or at the end of what shows of the last leaf above it, or the start of the first if it's above them all. What
/// an area that scrolls a text has moved out of sight doesn't show, so a leaf it cuts short ends where it's cut, and
/// one it cuts at its top starts there, for a point anywhere above it.
pub(super) fn spot_at(drawn: &[Drawn], position: Point<Pixels>) -> Option<Spot> {
    let mut nearest = None;
    for drawn in drawn {
        for (leaf, geometry) in &drawn.leaves {
            let (at, leaf, whole) = (drawn.at, *leaf, geometry.bounds());
            let shows = whole.intersect(&drawn.bounds);
            if position.y < shows.top() {
                // Cut at its top, it starts where what shows of it does, which is nearer than anything drawn above
                // the area that cuts it.
                if shows.top() > whole.top() {
                    return Some(Spot { at, leaf, byte: geometry.index_for_position(shows.origin) });
                }
                return nearest.or(Some(Spot { at, leaf, byte: 0 }));
            }
            if position.y <= shows.bottom() {
                let spot = Spot { at, leaf, byte: geometry.index_for_position(position) };
                // A table's cells sit side by side, so the one across from it is the one it's over, if it's over one.
                if shows.left() <= position.x && position.x <= shows.right() {
                    return Some(spot);
                }
                nearest = Some(spot);
                continue;
            }
            let cut = shows.bottom() < whole.bottom();
            let byte = if cut {
                geometry.index_for_position(point(shows.right(), shows.bottom() - px(1.)))
            } else {
                geometry.len()
            };
            nearest = Some(Spot { at, leaf, byte });
        }
    }
    nearest
}

/// The word `byte` is in or at the start of in `text`, or the character there if it isn't in one.
pub(super) fn word_around(text: &str, byte: usize) -> Range<usize> {
    let byte = byte.min(text.len());
    let in_a_word = |c: char| c.is_alphanumeric() || c == '_';
    let (before, after) = text.split_at(byte);
    let Some(here) = after.chars().next().or_else(|| before.chars().next_back()) else { return byte..byte };
    if !in_a_word(here) {
        return match after.chars().next() {
            Some(c) => byte..byte + c.len_utf8(),
            None => byte - here.len_utf8()..byte,
        };
    }
    let start = byte - before.chars().rev().take_while(|c| in_a_word(*c)).map(char::len_utf8).sum::<usize>();
    let end = byte + after.chars().take_while(|c| in_a_word(*c)).map(char::len_utf8).sum::<usize>();
    start..end
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::Spot;
    use super::{copied, word_around};
    use crate::prose::TextAt;

    #[test]
    fn a_copy_leaves_out_a_text_with_nothing_of_it_selected() {
        let (first, second) = (TextAt::only(1, 0), TextAt::only(2, 0));
        let texts = [(first, Arc::from(["one".to_string()])), (second, Arc::from(["two words".to_string()]))];
        // From just past the end of the first text into the second.
        let (start, end) = (Spot { at: first, leaf: 0, byte: 3 }, Spot { at: second, leaf: 0, byte: 3 });

        assert_eq!(copied(start, end, &texts), "two");
    }

    #[test]
    fn a_double_click_selects_the_word_its_in() {
        let text = "the quick_fox, jumps";

        assert_eq!(&text[word_around(text, 6)], "quick_fox");
        assert_eq!(&text[word_around(text, 4)], "quick_fox");
        assert_eq!(&text[word_around(text, 13)], ",");
        assert_eq!(&text[word_around(text, text.len())], "jumps");
    }
}
