//! Copies what a mirror shows into a [`Frame`], from Ghostty's render state, building again only the rows that
//! changed since the last copy.

use std::sync::Arc;

use libghostty_vt::color::Rgb as GhosttyRgb;
use libghostty_vt::render::{CellIteration, CellIterator, CursorVisualStyle, Dirty, RenderState, RowIterator};
use libghostty_vt::screen::CellWide;
use libghostty_vt::style::Underline;

use super::{Mirror, MirrorError};
use crate::Size;
use crate::drawn::{Background, Cursor, CursorShape, Frame, Glyph, GlyphText, Look, Rgb, Row, Selected};

#[cfg(test)]
mod tests;

/// What a mirror keeps to copy what it shows, made the first time it's copied, since the worker's never is.
pub(super) struct Drawing {
    render: RenderState<'static>,
    rows_in: RowIterator<'static>,
    cells_in: CellIterator<'static>,
    /// The rows of the last copy, which a row that hasn't changed is taken from.
    rows: Vec<Arc<Row>>,
    /// Holds a cell's text as it's read, so each cell doesn't allocate.
    text: String,
}

impl std::fmt::Debug for Drawing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Drawing").field("rows", &self.rows.len()).finish_non_exhaustive()
    }
}

impl Mirror {
    /// What the terminal shows now. A row that hasn't changed since the last copy is the same `Arc`.
    pub fn frame(&mut self) -> Result<Frame, MirrorError> {
        let drawing = match &mut self.drawing {
            Some(drawing) => drawing,
            None => self.drawing.insert(Drawing {
                render: RenderState::new()?,
                rows_in: RowIterator::new()?,
                cells_in: CellIterator::new()?,
                rows: Vec::new(),
                text: String::new(),
            }),
        };
        let Drawing { render, rows_in, cells_in, rows, text } = drawing;
        let snapshot = render.update(&self.terminal)?;
        let colors = snapshot.colors()?;
        let size = Size { cols: snapshot.cols()?, rows: snapshot.rows()? };
        let count = usize::from(size.rows);
        let all = snapshot.dirty()? == Dirty::Full || rows.len() != count;
        if rows.len() != count {
            rows.clear();
            rows.resize_with(count, Arc::default);
        }
        let mut each = rows_in.update(&snapshot)?;
        let mut y = 0;
        while let Some(row) = each.next() {
            // Ghostty counts a row whose selected cells changed as changed.
            if y < count && (all || row.dirty()?) {
                let mut cells = cells_in.update(row)?;
                let mut copy = copy_row(&mut cells, colors.foreground, colors.background, text)?;
                // Ghostty moves a selection's start off a wide character's second cell, but leaves its end on the
                // first, and its own renderer takes in the second. So does this.
                copy.selected = row.selection()?.map(|range| {
                    let end = range.end_x + u16::from(copy.wide_at(range.end_x));
                    Selected { col: range.start_x, cells: end.saturating_sub(range.start_x) + 1 }
                });
                rows[y] = Arc::new(copy);
                row.set_dirty(false)?;
            }
            y += 1;
        }
        let cursor = match (snapshot.cursor_visible()?, snapshot.cursor_viewport()?) {
            (true, Some(at)) => {
                // On either cell of a wide character, it covers both, as Ghostty draws it.
                let on_wide = |col| rows.get(usize::from(at.y)).is_some_and(|row| row.wide_at(col));
                let (col, wide) = if at.at_wide_tail { (at.x.saturating_sub(1), true) } else { (at.x, on_wide(at.x)) };
                let (shape, color) = (shape(snapshot.cursor_visual_style()?), snapshot.cursor_color()?.map(rgb));
                Some(Cursor { col, row: at.y, shape, color, wide })
            }
            _ => None,
        };
        snapshot.set_dirty(Dirty::Clean)?;
        Ok(Frame {
            size,
            rows: rows.clone(),
            cursor,
            foreground: rgb(colors.foreground),
            background: rgb(colors.background),
        })
    }
}

/// One row's cells, with a run of cells of the same background as one stretch.
fn copy_row(
    cells: &mut CellIteration<'_, '_>,
    foreground: GhosttyRgb,
    background: GhosttyRgb,
    text: &mut String,
) -> Result<Row, MirrorError> {
    let mut row = Row::default();
    let mut col = 0u16;
    while let Some(cell) = cells.next() {
        let at = col;
        col += 1;
        let wide = cell.raw_cell()?.wide()?;
        // What a wide character leaves in the cell after it, or at the end of a row it wrapped from.
        if matches!(wide, CellWide::SpacerTail | CellWide::SpacerHead) {
            continue;
        }
        let wide = wide == CellWide::Wide;
        // For every cell, since a cell a program cleared in a colour, as `bce` says it can, has that colour and no
        // style.
        let back = cell.bg_color()?;
        let (color, back, look, shown) = if cell.has_styling()? {
            let style = cell.style()?;
            let mut color = cell.fg_color()?.unwrap_or(foreground);
            let mut back = back;
            if style.inverse {
                (color, back) = (back.unwrap_or(background), Some(color));
            }
            let look = Look {
                bold: style.bold,
                italic: style.italic,
                faint: style.faint,
                underline: style.underline != Underline::None,
                strikethrough: style.strikethrough,
            };
            (color, back, look, !style.invisible)
        } else {
            (foreground, back, Look::default(), true)
        };
        if let Some(back) = back.map(rgb) {
            let cells = if wide { 2 } else { 1 };
            match row.backgrounds.last_mut() {
                Some(last) if last.color == back && last.col + last.cells == at => last.cells += cells,
                _ => row.backgrounds.push(Background { col: at, cells, color: back }),
            }
        }
        text.clear();
        cell.graphemes_utf8(text)?;
        let blank = text.is_empty() || text == " ";
        if !shown || (blank && !look.underline && !look.strikethrough) {
            continue;
        }
        let mut chars = text.chars();
        let text = match (chars.next(), chars.next()) {
            (Some(only), None) => GlyphText::Char(only),
            (None, _) => GlyphText::Char(' '),
            (Some(_), Some(_)) => GlyphText::Cluster(text.as_str().into()),
        };
        row.glyphs.push(Glyph { col: at, text, color: rgb(color), look, wide });
    }
    Ok(row)
}

/// Hollow is how a view draws a block while it isn't focused, which it knows itself, and any shape from a newer Ghostty
/// is drawn as a block too.
fn shape(style: CursorVisualStyle) -> CursorShape {
    if matches!(style, CursorVisualStyle::Bar) {
        CursorShape::Bar
    } else if matches!(style, CursorVisualStyle::Underline) {
        CursorShape::Underline
    } else {
        CursorShape::Block
    }
}

fn rgb(color: GhosttyRgb) -> Rgb {
    Rgb { r: color.r, g: color.g, b: color.b }
}
