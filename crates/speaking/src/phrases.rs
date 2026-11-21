//! Cutting a text into phrases as it comes: the first as soon as it can end, at a comma or a sentence's end, so the
//! voice starts at once, and each after at a sentence's end or a line's, so a place heard falls between whole phrases.

use std::ops::Range;

/// The longest a phrase can be, in bytes, before it's cut at the last space in it, so a place heard moves on often,
/// even through prose with no full stop.
pub(crate) const LONGEST: usize = 300;

/// One text being cut into phrases as it comes.
#[derive(Debug, Default)]
pub(crate) struct Cutting {
    text: String,
    /// Where the next phrase starts, in bytes.
    from: usize,
    /// No more of it is coming.
    ended: bool,
    /// The next phrase starts inside a fenced code block.
    fenced: bool,
    /// The phrase that ends the text has been cut.
    closed: bool,
}

/// A phrase: where it is in the text, and what kind it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Piece {
    pub(crate) range: Range<usize>,
    pub(crate) kind: Kind,
}

/// What a phrase is, which says how it's read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Read without Markdown's marks.
    Prose,
    /// A line of a fenced code block, read as it's written.
    Code,
    /// A fenced code block's fence, which says nothing.
    Fence,
}

impl Cutting {
    /// Takes the next of the text.
    pub(crate) fn push(&mut self, more: &str) {
        if !self.ended {
            self.text.push_str(more);
        }
    }

    /// No more of the text is coming, so what's left is a phrase however it ends.
    pub(crate) fn end(&mut self) {
        self.ended = true;
    }

    /// Whether the phrase that ends it has been cut. A text that ended after its last phrase was cut, or ended empty,
    /// has one more, with nothing in it, so its end is heard in its turn.
    pub(crate) fn finished(&self) -> bool {
        self.closed
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// The next phrase, once all of it has come: once what follows its end has come, or the text has ended.
    pub(crate) fn next(&mut self) -> Option<Piece> {
        let piece = self.find()?;
        self.from = piece.range.end;
        self.closed = self.ended && self.from == self.text.len();
        Some(piece)
    }

    fn find(&mut self) -> Option<Piece> {
        let rest = &self.text[self.from..];
        if rest.is_empty() {
            return (self.ended && !self.closed).then_some(Piece { range: self.from..self.from, kind: Kind::Prose });
        }
        // Blank lines go with the phrase after them, so they're never a phrase of their own.
        let blank = blank_lines(rest);
        let start = self.from + blank;
        let line = &self.text[start..];
        let newline = line.find('\n');
        let whole_line = newline.map(|at| start + at + 1);
        if is_fence(line) {
            // A fence's line says only which language follows, so it's a phrase alone, and nothing is said of it.
            let end = whole_line.or(self.ended.then_some(self.text.len()))?;
            self.fenced = !self.fenced;
            return Some(Piece { range: self.from..end, kind: Kind::Fence });
        }
        if self.fenced {
            // Code is read a line at a time, as it's written.
            let end = whole_line.or(self.ended.then_some(self.text.len()))?;
            return Some(Piece { range: self.from..capped(&self.text, self.from, end), kind: Kind::Code });
        }
        let first = self.from == 0;
        let line_end = whole_line.unwrap_or(self.text.len());
        let end = match sentence_end(&self.text[start..line_end], first) {
            Some(at) => start + at,
            None if whole_line.is_some() || self.ended => line_end,
            // The phrase may end further on, once more comes, unless it's already too long to wait for.
            None if line_end - self.from > LONGEST => line_end,
            None => return None,
        };
        Some(Piece { range: self.from..capped(&self.text, self.from, end), kind: Kind::Prose })
    }
}

/// How many bytes of `text` are whole lines with nothing but spaces in them, at its start.
fn blank_lines(text: &str) -> usize {
    let mut at = 0;
    while let Some(newline) = text[at..].find('\n') {
        if !text[at..at + newline].trim().is_empty() {
            break;
        }
        at += newline + 1;
    }
    at
}

/// Whether `line` opens or closes a fenced code block.
fn is_fence(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("```") || line.starts_with("~~~")
}

/// Where in `line` the first phrase in it ends, with the spaces after it: after a sentence's end, and for the text's
/// first phrase a comma, a semicolon or a colon too, but only once what follows it has come and it's a space. Closing
/// quotes and brackets go with the end before them. A list's number, such as `2. `, isn't an end.
fn sentence_end(line: &str, first: bool) -> Option<usize> {
    let marks: &[char] = if first { &['.', '!', '?', ',', ';', ':'] } else { &['.', '!', '?'] };
    let mut chars = line.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if !marks.contains(&c) || (c == '.' && line[..at].trim_start().chars().all(|d| d.is_ascii_digit())) {
            continue;
        }
        let mut end = at + c.len_utf8();
        while let Some(&(after, closing)) = chars.peek()
            && matches!(closing, '"' | '\'' | ')' | ']' | '*' | '_' | '`' | '”' | '’')
        {
            end = after + closing.len_utf8();
            chars.next();
        }
        let spaces = line[end..].len() - line[end..].trim_start().len();
        if spaces > 0 {
            return Some(end + spaces);
        }
    }
    None
}

/// Where a phrase from `from` to `end` in `text` ends once it's no longer than [`LONGEST`]: at the last space before
/// that, or at that, between characters, if there's none.
fn capped(text: &str, from: usize, end: usize) -> usize {
    if end - from <= LONGEST {
        return end;
    }
    let mut most = from + LONGEST;
    while !text.is_char_boundary(most) {
        most -= 1;
    }
    text[from..most].rfind(' ').map_or(most, |space| from + space + 1)
}
