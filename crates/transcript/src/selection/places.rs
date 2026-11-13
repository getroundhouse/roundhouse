//! Places in the texts a selection is made in: where a point in the window falls among the texts drawn, what's
//! selected of a text between two places, as a copy has it, and the word and the line around a place.

use std::borrow::Cow;
use std::ops::Range;

use gpui_kit::{Pixels, Point, point, px};

use super::{Drawn, Spot};
use crate::texts::{ATOM, Leaves, TextAt, TextKey};

/// What's selected from `start` to `end` of `texts`, as text: what's selected of each leaf a line apart, and of each
/// text a blank line apart, leaving out a leaf, or a text, that has none of it selected.
pub(super) fn copied(start: Spot, end: Spot, texts: &[(TextAt, Leaves)]) -> String {
    let mut copied = Vec::new();
    for (at, leaves) in texts {
        let parts: Vec<Cow<'_, str>> = selected(start, end, *at, leaves.each())
            .filter(|(_, range)| !range.is_empty())
            .filter_map(|(leaf, range)| copied_of(leaves, leaf, range))
            .collect();
        if !parts.is_empty() {
            copied.push(parts.join("\n"));
        }
    }
    copied.join("\n\n")
}

/// What a copy has of `range` of the leaf `leaf` of `leaves`, if the leaf has it: what it shows, but for an image in
/// Markdown, which is an [`ATOM`] there.
fn copied_of(leaves: &Leaves, leaf: u64, range: Range<usize>) -> Option<Cow<'_, str>> {
    let text = leaves.leaf(leaf)?.get(range)?;
    Some(match leaves {
        Leaves::Markdown(_) => Cow::Owned(text.replace(ATOM, "")),
        Leaves::Plain(_) => Cow::Borrowed(text),
    })
}

/// What's selected from `start` to `end` of the text at `at`, whose leaves say `leaves`, by leaf. An end keeps its
/// byte while its text is parsed again, and a leaf can say other than it did when the end was put there, so a byte
/// past a leaf's end, or inside one of its characters, is taken as the nearest place before, as [`word_around`] takes
/// it. So what's painted as selected is what a copy has.
pub(super) fn selected<'a>(
    start: Spot,
    end: Spot,
    at: TextAt,
    leaves: impl IntoIterator<Item = &'a str> + 'a,
) -> impl Iterator<Item = (u64, Range<usize>)> + 'a {
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
        Some((leaf, text.floor_char_boundary(low)..text.floor_char_boundary(high)))
    })
}

/// The text drawn in this frame that `position` is on, if it's on one that shows any of its leaves.
pub(super) fn text_under(drawn: &[Drawn], position: Point<Pixels>) -> Option<&Drawn> {
    drawn.iter().find(|drawn| drawn.bounds.contains(&position) && !drawn.leaves.is_empty())
}

/// The place in what shows of the texts drawn in this frame nearest `position`, and the text it's in: in the leaf
/// under it, or across from it, or at the end of what shows of the last leaf above it, or the start of the first if
/// it's above them all. A point on a text is in that text, so one in the space its element gives it above its first
/// leaf, such as a block's around what a command printed, is at that leaf's start, rather than at the end of the text
/// above. What an area that scrolls a text has moved out of sight doesn't show, so a leaf it cuts short ends where it's
/// cut, and one it cuts at its top starts there, for a point anywhere above it.
pub(super) fn spot_at(drawn: &[Drawn], position: Point<Pixels>) -> Option<(Spot, &TextKey)> {
    let on = text_under(drawn, position);
    let mut nearest = None;
    for drawn in on.map_or(drawn, std::slice::from_ref) {
        let text = &drawn.text;
        for (leaf, geometry) in &drawn.leaves {
            let (at, leaf, whole) = (drawn.at, *leaf, geometry.bounds());
            let shows = whole.intersect(&drawn.bounds);
            if position.y < shows.top() {
                // Cut at its top, it starts where what shows of it does, which is nearer than anything drawn above
                // the area that cuts it.
                if shows.top() > whole.top() {
                    return Some((Spot { at, leaf, byte: geometry.index_for_position(shows.origin, shows) }, text));
                }
                return nearest.or(Some((Spot { at, leaf, byte: 0 }, text)));
            }
            if position.y <= shows.bottom() {
                let spot = Spot { at, leaf, byte: geometry.index_for_position(position, shows) };
                // A table's cells sit side by side, so the one across from it is the one it's over, if it's over one.
                if shows.left() <= position.x && position.x <= shows.right() {
                    return Some((spot, text));
                }
                nearest = Some((spot, text));
                continue;
            }
            let cut = shows.bottom() < whole.bottom();
            let end = point(shows.right(), shows.bottom() - px(1.));
            let byte = if cut { geometry.index_for_position(end, shows) } else { geometry.len() };
            nearest = Some((Spot { at, leaf, byte }, text));
        }
    }
    nearest
}

/// The word `byte` is in or at the start of in `text`, or the character there if it isn't in one. `byte` is a place in
/// the text as it was drawn in the last frame, and the text can say other than it did then, once a parse of it on
/// another thread has landed since, so a byte past its end, or inside one of its characters, is taken as the nearest
/// place before.
pub(super) fn word_around(text: &str, byte: usize) -> Range<usize> {
    let byte = text.floor_char_boundary(byte);
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

/// The line `byte` is in, in `text`, without the line break that ends it, which a triple-click in plain text selects,
/// so it's a line of what a command printed however that's laid out. A byte past its end, or inside one of its
/// characters, is taken as the nearest place before, as [`word_around`] takes it.
pub(super) fn line_around(text: &str, byte: usize) -> Range<usize> {
    let byte = text.floor_char_boundary(byte);
    let start = text[..byte].rfind('\n').map_or(0, |at| at + 1);
    start..text[byte..].find('\n').map_or(text.len(), |at| byte + at)
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, TestAppContext, point, px, size};
    use protocol::model::Seq;

    use super::super::test_layouts::in_pieces;
    use super::super::{Drawn, Geometry, Spot};
    use super::{copied, line_around, selected, spot_at, text_under, word_around};
    use crate::texts::{ATOM, Leaves, TextAt, TextKey};

    /// The leaves of Markdown that says `leaves`.
    fn markdown(leaves: &[&str]) -> Leaves {
        Leaves::Markdown(leaves.iter().map(ToString::to_string).collect())
    }

    #[test]
    fn a_copy_leaves_out_a_text_with_nothing_of_it_selected() {
        let (first, second) = (TextAt::only(Seq::new(1), 0), TextAt::only(Seq::new(2), 0));
        let texts = [(first, markdown(&["one"])), (second, markdown(&["two words"]))];
        // From just past the end of the first text into the second.
        let (start, end) = (Spot { at: first, leaf: 0, byte: 3 }, Spot { at: second, leaf: 0, byte: 3 });

        assert_eq!(copied(start, end, &texts), "two");
    }

    /// The leaf showed the backtick that opens its code span when the selection ended before the dash, and the one
    /// that closes it has come since, so the leaf is a byte shorter there and the end's byte is inside the dash, which
    /// is three bytes.
    #[test]
    fn a_copy_of_a_leaf_that_s_changed_under_an_end_ends_at_the_character_before() {
        let at = TextAt::only(Seq::new(1), 0);
        let texts = [(at, markdown(&["Use foo — bar"]))];
        let (start, end) = (Spot { at, leaf: 0, byte: 0 }, Spot { at, leaf: 0, byte: 9 });
        let (inside, after) = (Spot { at, leaf: 0, byte: 10 }, Spot { at, leaf: 0, byte: 15 });

        assert_eq!(copied(start, end, &texts), "Use foo ");
        assert_eq!(copied(inside, after, &texts), "— bar", "and one that starts inside it starts with it");
    }

    /// Each leaf of a text a line apart, such as each paragraph of a reply, and each text a blank line apart, such as
    /// a command and what it printed. An image in Markdown is an [`ATOM`] in its leaf, which a copy leaves out, and one
    /// in plain text is a character it says.
    #[test]
    fn a_copy_has_each_leaf_a_line_apart_each_text_a_blank_line_apart_and_an_atom_only_where_it_s_said() {
        let (reply, command, output) =
            (TextAt::only(Seq::new(1), 0), TextAt::only(Seq::new(2), 0), TextAt::only(Seq::new(2), 1));
        let printed = format!("one\nfound {ATOM} in it");
        let texts = [
            (reply, markdown(&[&format!("See {ATOM}here"), "Then this"])),
            (command, Leaves::Plain("grep -r x".into())),
            (output, Leaves::Plain(printed.clone().into())),
        ];
        let (start, end) = (Spot { at: reply, leaf: 0, byte: 0 }, Spot { at: output, leaf: 0, byte: printed.len() });

        assert_eq!(copied(start, end, &texts), format!("See here\nThen this\n\ngrep -r x\n\n{printed}"));
    }

    /// What's selected of a text runs from one end to the other: from the first end, in its leaf, to the last, in
    /// its, through every leaf between. A text between the ends is selected whole, and one outside them not at all.
    #[test]
    fn what_s_selected_of_a_text_runs_from_one_end_to_the_other() {
        let (first, between, after) =
            (TextAt::only(Seq::new(1), 0), TextAt::only(Seq::new(2), 0), TextAt::only(Seq::new(3), 0));
        let leaves = ["one", "", "three"];
        let (start, end) = (Spot { at: first, leaf: 0, byte: 1 }, Spot { at: first, leaf: 2, byte: 0 });
        let (from_between, to_after) = (Spot { at: first, leaf: 2, byte: 0 }, Spot { at: after, leaf: 0, byte: 0 });

        assert_eq!(selected(start, end, first, leaves).collect::<Vec<_>>(), [(0, 1..3), (1, 0..0), (2, 0..0)]);
        assert_eq!(
            selected(from_between, to_after, between, leaves).collect::<Vec<_>>(),
            [(0, 0..3), (1, 0..0), (2, 0..5)]
        );
        assert_eq!(selected(start, end, after, leaves).count(), 0, "and nothing of a text outside them");
    }

    #[test]
    fn a_triple_click_in_plain_text_selects_the_line_it_s_in() {
        let text = "one\ntwo\n\nthree";
        let line = |byte| &text[line_around(text, byte)];

        assert_eq!(line(5), "two");
        assert_eq!(line(4), "two", "from its start");
        assert_eq!(line(7), "two", "to its end");
        assert_eq!(line(8), "", "and a blank line is nothing");
        assert_eq!(line(100), "three", "and past its end is its last line");
    }

    /// A text that shows none of its leaves, such as an output whose block shows only the space it gives it at the
    /// list's edge, isn't one to be on: a press there is on no text, as one between two texts is, and a drag over it
    /// goes to the nearest place in what shows of the texts drawn. Here that's in the text above, which an area cuts
    /// short in the space between its two pieces, so it's the end of the first, not a place in the second, which
    /// doesn't show, as it is for a point on the text above just over where it's cut.
    #[gpui_kit::test]
    fn a_text_that_shows_none_of_its_leaves_is_not_one_to_be_on(cx: &mut TestAppContext) {
        let (pieces, layouts) = in_pieces(cx.add_empty_window(), &["abc", "de"], 6.);
        let cut = (layouts[0].bounds().bottom() + layouts[1].bounds().top()) / 2.;
        let shows = Bounds::from_corners(pieces.bounds().origin, point(pieces.bounds().right(), cut));
        let (above, edge) = (TextAt::only(Seq::new(1), 0), TextAt::only(Seq::new(2), 0));
        let leaves = vec![(0, Geometry::Plain(pieces))];
        let cut_short = Drawn { text: TextKey::named("above"), at: above, bounds: shows, area: None, leaves };
        let bounds = Bounds::new(point(px(0.), cut), size(px(200.), px(8.)));
        let edge = Drawn { text: TextKey::named("edge"), at: edge, bounds, area: None, leaves: vec![] };
        let drawn = [cut_short, edge];
        let (inside, just_over) = (point(px(10.), cut + px(4.)), point(px(150.), cut - px(1.)));
        let found = |at| spot_at(&drawn, at).map(|(spot, text)| (spot.at, spot.byte, text.clone()));

        assert!(text_under(&drawn, inside).is_none());
        assert_eq!(found(inside), Some((above, 3, TextKey::named("above"))), "the end of what shows of the first");
        assert_eq!(found(just_over), Some((above, 3, TextKey::named("above"))), "and the same just over the cut");
    }

    #[test]
    fn a_double_click_selects_the_word_its_in() {
        let text = "the quick_fox, jumps";

        assert_eq!(&text[word_around(text, 6)], "quick_fox");
        assert_eq!(&text[word_around(text, 4)], "quick_fox");
        assert_eq!(&text[word_around(text, 13)], ",");
        assert_eq!(&text[word_around(text, text.len())], "jumps");
    }

    #[test]
    fn a_double_click_at_a_place_the_text_no_longer_has_selects_the_word_before_it() {
        let text = "naïve café";

        assert_eq!(&text[word_around(text, 3)], "naïve", "inside the ï, which is two bytes");
        assert_eq!(&text[word_around(text, text.len() + 4)], "café", "past its end");
    }
}
