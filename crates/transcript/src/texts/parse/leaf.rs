//! One leaf of a Markdown text: what it shows, as the kit lays it out, where anything drawn inline that isn't text is
//! one [`ATOM`], and what each of those says, which find looks in, and a copy has, in its place. So a reference drawn
//! as a chip is found and copied as the words of the link it was, such as `#1021`. An image says the [`ATOM`] itself, which
//! no needle holds, so a match doesn't run across it, and which a copy leaves out.

use std::borrow::Cow;
use std::ops::Range;

use super::markdown::ATOM;

/// What one leaf shows, and what each [`ATOM`] in it says, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Leaf {
    shows: String,
    /// One for each [`ATOM`] in `shows`. One the text itself has says itself.
    atoms: Box<[Box<str>]>,
}

impl Leaf {
    /// A leaf that shows `shows`, where each [`ATOM`] says what `atoms` has for it, in order, and itself past their
    /// end.
    pub(crate) fn new(shows: String, atoms: impl IntoIterator<Item = Box<str>>) -> Self {
        let mut atoms = atoms.into_iter();
        let atoms = shows.matches(ATOM).map(|atom| atoms.next().unwrap_or_else(|| Box::from(atom))).collect();
        Self { shows, atoms }
    }

    /// A leaf of only text, such as a code block's, where an [`ATOM`] is one its text has, which says itself.
    pub(crate) fn text(shows: String) -> Self {
        let atoms = shows.matches(ATOM).map(Box::from).collect();
        Self { shows, atoms }
    }

    /// What it shows, as a selection's bytes and find's marks are in it.
    pub(crate) fn shows(&self) -> &str {
        &self.shows
    }

    /// What `range` of what it shows says, if it's in it.
    pub(crate) fn says(&self, range: Range<usize>) -> Option<Cow<'_, str>> {
        let shown = self.shows.get(range.clone())?;
        if !shown.contains(ATOM) {
            return Some(Cow::Borrowed(shown));
        }
        let mut atoms = self.atoms[self.shows[..range.start].matches(ATOM).count()..].iter();
        let mut says = String::with_capacity(shown.len());
        for (ix, piece) in shown.split(ATOM).enumerate() {
            if ix > 0 {
                says.push_str(atoms.next().map_or("", |atom| atom));
            }
            says.push_str(piece);
        }
        Some(Cow::Owned(says))
    }

    /// All it says.
    pub(crate) fn said(&self) -> Cow<'_, str> {
        self.says(0..self.shows.len()).unwrap_or_default()
    }

    /// Where `find` finds what it looks for in all it says, as the ranges of what it shows that [`Leaf::shown`] has.
    pub(crate) fn found(&self, find: impl FnOnce(&str) -> Vec<Range<usize>>) -> Vec<Range<usize>> {
        find(&self.said()).into_iter().map(|range| self.shown(range)).collect()
    }

    /// Where `range` of what it says, as [`Leaf::said`] has it, is in what it shows. One that starts or ends inside
    /// what an [`ATOM`] says takes in all of it.
    pub(crate) fn shown(&self, range: Range<usize>) -> Range<usize> {
        if self.atoms.is_empty() {
            return range;
        }
        self.shown_at(range.start, false)..self.shown_at(range.end, true)
    }

    /// Where `said`, a byte of what it says, is in what it shows, as the start of a range, or its end if `end`.
    fn shown_at(&self, said: usize, end: bool) -> usize {
        let (mut shown_from, mut said_from) = (0, 0);
        for ((shown_at, _), atom) in self.shows.match_indices(ATOM).zip(&self.atoms) {
            let said_at = said_from + (shown_at - shown_from);
            let said_end = said_at + atom.len();
            let inside = if end { said_at < said && said <= said_end } else { said_at <= said && said < said_end };
            if inside {
                return if end { shown_at + ATOM.len_utf8() } else { shown_at };
            }
            if said < said_at || (end && said == said_at) {
                break;
            }
            (shown_from, said_from) = (shown_at + ATOM.len_utf8(), said_end);
        }
        shown_from + (said - said_from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf() -> Leaf {
        Leaf::new(format!("Fixed in {ATOM} and {ATOM}{ATOM}."), ["#12".into(), "".into(), "o/r#7".into()])
    }

    #[test]
    fn what_a_leaf_says_has_each_atom_as_what_it_says() {
        let leaf = leaf();
        assert_eq!(leaf.said(), "Fixed in #12 and o/r#7.");
        let second = leaf.shows().find(" and").unwrap();
        assert_eq!(leaf.says(second..leaf.shows().len()).unwrap(), " and o/r#7.", "counting the atoms before it");
        assert_eq!(Leaf::text(format!("a {ATOM} b")).said(), format!("a {ATOM} b"), "one the text has says itself");
    }

    #[test]
    fn what_a_leaf_says_is_where_it_shows_it_and_inside_an_atom_is_all_of_it() {
        let leaf = leaf();
        let shown = |said: &str| {
            let at = leaf.said().find(said).unwrap();
            let range = leaf.shown(at..at + said.len());
            leaf.shows()[range].to_string()
        };
        assert_eq!(shown("Fixed"), "Fixed");
        assert_eq!(shown("#12"), ATOM.to_string());
        assert_eq!(shown("in #1"), format!("in {ATOM}"));
        assert_eq!(shown("2 and"), format!("{ATOM} and"));
        assert_eq!(shown(" and o"), format!(" and {ATOM}{ATOM}"), "and an atom that says nothing on the way");
        assert_eq!(shown("#7."), format!("{ATOM}."));
    }
}
