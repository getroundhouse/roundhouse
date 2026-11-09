//! Lines of a file to show, each with its number in the file, and marked when a change added or removed it. A read
//! and a search show a file's lines as they are, and a change shows what it did to them.

use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mark {
    Same,
    Removed,
    Added,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NumberedLine {
    pub(crate) mark: Mark,
    /// Its number in the file: before the change for a removed line, and after it otherwise. Unknown when a change
    /// was worked out from the call.
    pub(crate) number: Option<u32>,
    pub(crate) text: String,
    /// The part of the line that changed, when it's a line changed in place.
    pub(crate) changed: Option<Range<usize>>,
}

/// Lines that are next to each other in a file. Sections are shown apart, with a gap between them.
pub(crate) type Section = Vec<NumberedLine>;

impl NumberedLine {
    pub(crate) fn new(mark: Mark, number: Option<u32>, text: &str) -> Self {
        Self { mark, number, text: text.to_string(), changed: None }
    }

    /// A line as it is in the file, such as one a read sent back.
    pub(crate) fn same(number: u32, text: &str) -> Self {
        Self::new(Mark::Same, Some(number), text)
    }
}

/// How many lines a change added and removed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) added: usize,
    pub(crate) removed: usize,
}

impl Counts {
    pub(crate) fn of(marks: impl IntoIterator<Item = Mark>) -> Self {
        let mut counts = Self::default();
        for mark in marks {
            match mark {
                Mark::Added => counts.added += 1,
                Mark::Removed => counts.removed += 1,
                Mark::Same => {}
            }
        }
        counts
    }
}
