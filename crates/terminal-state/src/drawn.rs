//! A plain copy of what a terminal shows, for a view to draw. It holds nothing of Ghostty's, so it can leave the thread
//! the terminal lives on, and a row that hasn't changed is the same `Arc` as in the copy before, so a view can tell.

use std::sync::Arc;

use crate::Size;

/// What a terminal shows, as it was when it was copied.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub size: Size,
    /// Each row in sight, top first, which is the screen's unless the view has scrolled back through its history.
    pub rows: Vec<Arc<Row>>,
    /// Where the cursor is, unless it's hidden or scrolled out of sight.
    pub cursor: Option<Cursor>,
    /// The colours of a cell that sets none.
    pub foreground: Rgb,
    pub background: Rgb,
}

/// One row of cells.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    /// The stretches of cells whose background isn't the terminal's, in order.
    pub backgrounds: Vec<Background>,
    /// Each cell that draws something, in order, which leaves out a blank one with no line through or under it.
    pub glyphs: Vec<Glyph>,
}

impl Row {
    /// Its text, with a space for each cell that draws nothing before the last that does, for a client that shows text
    /// alone, or a test.
    #[must_use]
    pub fn text(&self) -> String {
        let mut text = String::new();
        let mut next = 0;
        for glyph in &self.glyphs {
            text.extend(std::iter::repeat_n(' ', usize::from(glyph.col.saturating_sub(next))));
            match &glyph.text {
                GlyphText::Char(one) => text.push(*one),
                GlyphText::Cluster(cluster) => text.push_str(cluster),
            }
            next = glyph.col + if glyph.wide { 2 } else { 1 };
        }
        text
    }

    /// Whether a wide character starts at column `col`.
    pub(crate) fn wide_at(&self, col: u16) -> bool {
        self.glyphs.binary_search_by_key(&col, |glyph| glyph.col).is_ok_and(|at| self.glyphs[at].wide)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Background {
    pub col: u16,
    /// How many cells it covers.
    pub cells: u16,
    pub color: Rgb,
}

/// What one cell draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub col: u16,
    pub text: GlyphText,
    pub color: Rgb,
    pub look: Look,
    /// Whether it takes two cells, as most emoji and East Asian characters do.
    pub wide: bool,
}

/// A cell's character, or the cluster of them it shows as one, such as a letter with a combining accent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlyphText {
    Char(char),
    Cluster(Box<str>),
}

/// How a cell's text is drawn, apart from its colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[expect(clippy::struct_excessive_bools, reason = "each is a style of its own, which a program sets or leaves off")]
pub struct Look {
    pub bold: bool,
    pub italic: bool,
    /// Dimmed.
    pub faint: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub col: u16,
    pub row: u16,
    pub shape: CursorShape,
    /// The colour the program set for it, if it set one.
    pub color: Option<Rgb>,
    /// Over either cell of a wide character, so it covers both, and `col` is the first.
    pub wide: bool,
}

/// The cursor's shape, as the program set it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Bar,
    Underline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// As `0xRRGGBB`.
    #[must_use]
    pub fn hex(self) -> u32 {
        (u32::from(self.r) << 16) | (u32::from(self.g) << 8) | u32::from(self.b)
    }
}
