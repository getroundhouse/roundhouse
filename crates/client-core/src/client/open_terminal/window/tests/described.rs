//! A frame written out a line a row, to compare with what Ghostty drew of the same output, recorded when the client
//! still kept a copy of Ghostty of its own and checked equal to it.

use std::fmt::Write as _;

use crate::drawn::{Frame, GlyphText, Row};

/// `frame` as text: its size, its colours and its cursor, then each row that draws anything, by its number.
pub(super) fn described(frame: &Frame) -> String {
    let mut text = format!(
        "{}x{} on #{:06x} in #{:06x}",
        frame.size.cols,
        frame.size.rows,
        frame.background.hex(),
        frame.foreground.hex()
    );
    if let Some(cursor) = frame.cursor {
        let _ = write!(text, ", cursor at {},{} {:?}", cursor.col, cursor.row, cursor.shape);
        if let Some(color) = cursor.color {
            let _ = write!(text, " #{:06x}", color.hex());
        }
        if cursor.wide {
            text.push_str(" wide");
        }
    }
    text.push('\n');
    for (at, row) in frame.rows.iter().enumerate() {
        if **row != Row::default() {
            let _ = writeln!(text, "{at}:{}", self::row(row));
        }
    }
    text
}

/// Each stretch of background as `[col+cells #colour]`, each glyph as its column, text, colour and look, and what's
/// selected, from its first column for so many cells.
fn row(row: &Row) -> String {
    let mut text = String::new();
    for background in &row.backgrounds {
        let _ = write!(text, " [{}+{} #{:06x}]", background.col, background.cells, background.color.hex());
    }
    for glyph in &row.glyphs {
        let shown = match &glyph.text {
            GlyphText::Char(one) => one.to_string(),
            GlyphText::Cluster(cluster) => cluster.to_string(),
        };
        let _ = write!(text, " {}{shown:?}#{:06x}", glyph.col, glyph.color.hex());
        let look = glyph.look;
        let mut first = true;
        for (on, letter) in [
            (look.bold, "bold"),
            (look.italic, "italic"),
            (look.faint, "faint"),
            (look.underline, "underline"),
            (look.strikethrough, "through"),
            (glyph.wide, "wide"),
        ] {
            if on {
                text.push(if first { ':' } else { ',' });
                text.push_str(letter);
                first = false;
            }
        }
    }
    if let Some(selected) = row.selected {
        let _ = write!(text, " selected {}+{}", selected.col, selected.cells);
    }
    text
}
