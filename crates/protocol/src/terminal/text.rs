//! A terminal's cells by number, for what a client selects: the ends of a word or a line, which the worker finds by
//! Ghostty's rules, and the text between two cells, which it copies with Ghostty's formatter, so what's highlighted
//! is what's copied, joined and trimmed as Ghostty copies it.

use serde::{Deserialize, Serialize};

use super::{Numbering, RowNumber};

/// The most bytes of text a reply carries, which is about a frame's worth, as a picture's frame is.
pub const MOST_TEXT_BYTES: usize = 64 * 1024;

/// A cell of a terminal: its row's number, and its column from the left, from 0.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TerminalPoint {
    pub row: RowNumber,
    pub col: u16,
}

/// The cells from one to another, both in it, in reading order.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalRange {
    pub from: TerminalPoint,
    pub to: TerminalPoint,
}

/// What a client asks the ends of, as a double click selects a word and a triple click a line.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Bounds {
    Word,
    /// A line as the terminal wrapped it, its rows joined, without the spaces at either end.
    Line,
    /// One a newer peer knows, which has no ends.
    #[serde(other)]
    Unknown,
}

/// The ends of a word or a line, in the numbering the terminal's rows are in now. None once they're in another
/// numbering than the one asked about, or at a cell the terminal doesn't keep, or that has no word.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TerminalBoundsReply {
    pub numbering: Numbering,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<TerminalRange>,
}

/// The text of cells, as Ghostty copies it: a line the terminal wrapped joined again, and the spaces at the end of
/// each line left out. At most [`MOST_TEXT_BYTES`] of it, with where the rest starts, if there's more. A page ends
/// where a line does, unless one line is more than a page, so the pages make the same text as one would, each after
/// the last with a newline between, unless the earlier `joins` the next. Its text is kept out of `Debug`, since what
/// a terminal shows can be a secret.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct TerminalTextReply {
    /// The numbering the terminal's rows are in now. The text is empty once that's another than the one asked about.
    pub numbering: Numbering,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rest: Option<TerminalPoint>,
    /// Whether the rest goes on the same line, as it does only from a line that's more than a page.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub joins: bool,
}

impl std::fmt::Debug for TerminalTextReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalTextReply")
            .field("numbering", &self.numbering)
            .field("text", &format_args!("{} bytes", self.text.len()))
            .field("rest", &self.rest)
            .field("joins", &self.joins)
            .finish()
    }
}
