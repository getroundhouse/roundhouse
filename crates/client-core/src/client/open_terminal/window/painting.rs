//! A row as the view draws it, from a row of a picture: each cell's colours from the palette and the view's own, and
//! the glyphs and backgrounds it paints, as Ghostty draws them.

use std::sync::Arc;

use protocol::terminal::{
    CellAttributes, CellColor, CellStyle, DEFAULT_PALETTE, ScreenState, Span, TerminalCursor, TerminalRow,
};

use crate::drawn::{Background, Cursor, CursorShape, Glyph, GlyphText, Look, Rgb, Row};

/// The colours a row is drawn in: the palette, with what the program changed of it, and the colours of a cell that sets
/// none, the program's where it set them, and the view's otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Colors {
    palette: Box<[Rgb; 256]>,
    pub(super) foreground: Rgb,
    pub(super) background: Rgb,
    cursor: Option<Rgb>,
}

/// The colours of a cell that sets none until the view says its own, which are Ghostty's.
pub(super) const GHOSTTYS_COLORS: (Rgb, Rgb) = (Rgb { r: 0xff, g: 0xff, b: 0xff }, Rgb { r: 0, g: 0, b: 0 });

impl Colors {
    pub(super) fn new(state: &ScreenState, (foreground, background): (Rgb, Rgb)) -> Self {
        let mut palette = Box::new(DEFAULT_PALETTE.map(|[r, g, b]| Rgb { r, g, b }));
        for changed in &state.palette {
            if let Some(color) = rgb(&palette, changed.color) {
                palette[usize::from(changed.index)] = color;
            }
        }
        let mut foreground = state.foreground.and_then(|color| rgb(&palette, color)).unwrap_or(foreground);
        let mut background = state.background.and_then(|color| rgb(&palette, color)).unwrap_or(background);
        if state.reverse {
            (foreground, background) = (background, foreground);
        }
        let cursor = state.cursor_color.and_then(|color| rgb(&palette, color));
        Self { palette, foreground, background, cursor }
    }

    fn of(&self, color: Option<CellColor>) -> Option<Rgb> {
        color.and_then(|color| rgb(&self.palette, color))
    }

    /// The cursor on `row`, its row on the screen, as the view draws it.
    pub(super) fn cursor(&self, cursor: TerminalCursor, row: u16, drawn: &Row) -> Cursor {
        let shape = match cursor.shape {
            protocol::terminal::CursorShape::Bar => CursorShape::Bar,
            protocol::terminal::CursorShape::Underline => CursorShape::Underline,
            // Hollow is how a view draws a block while it isn't focused, which it knows itself, and a shape from a
            // newer peer is drawn as a block too.
            protocol::terminal::CursorShape::Block
            | protocol::terminal::CursorShape::BlockHollow
            | protocol::terminal::CursorShape::Unknown => CursorShape::Block,
        };
        Cursor { col: cursor.col, row, shape, color: self.cursor, wide: drawn.wide_at(cursor.col) }
    }

    /// What `row` draws, with nothing selected.
    pub(super) fn paint(&self, row: &TerminalRow) -> Arc<Row> {
        let mut painted = Row::default();
        let mut col = 0u16;
        for span in &row.spans {
            col = span.col.unwrap_or(col);
            let style = span.style.and_then(|at| row.styles.get(usize::from(at))).copied().unwrap_or_default();
            let cell = self.cell(&style);
            let width = if span.wide { 2 } else { 1 };
            if span.text.is_empty() {
                let cells = span.blank;
                cell.background(&mut painted, col, cells);
                // A blank cell draws only a line through it or under it.
                if cell.lined() {
                    for at in (0..cells).step_by(usize::from(width)) {
                        cell.glyph(&mut painted, col + at, GlyphText::Char(' '), span.wide);
                    }
                }
                col += cells;
            } else if span.cluster {
                cell.background(&mut painted, col, width);
                cell.glyph(&mut painted, col, GlyphText::Cluster(span.text.as_str().into()), span.wide);
                col += width;
            } else {
                col = chars(&mut painted, span, &cell, col, width);
            }
        }
        Arc::new(painted)
    }

    /// How a cell in `style` is drawn.
    fn cell(&self, style: &CellStyle) -> Cell {
        let attributes = style.attributes;
        let mut color = self.of(style.foreground).unwrap_or(self.foreground);
        let mut back = self.of(style.background);
        if attributes.contains(CellAttributes::INVERSE) {
            (color, back) = (back.unwrap_or(self.background), Some(color));
        }
        let look = Look {
            bold: attributes.contains(CellAttributes::BOLD),
            italic: attributes.contains(CellAttributes::ITALIC),
            faint: attributes.contains(CellAttributes::FAINT),
            underline: style.underline.is_some(),
            strikethrough: attributes.contains(CellAttributes::STRIKETHROUGH),
        };
        Cell { color, back, look, shown: !attributes.contains(CellAttributes::INVISIBLE) }
    }
}

/// How the cells of a span are drawn.
struct Cell {
    color: Rgb,
    back: Option<Rgb>,
    look: Look,
    shown: bool,
}

impl Cell {
    fn lined(&self) -> bool {
        self.look.underline || self.look.strikethrough
    }

    /// Its background over `cells` from `col`, joined to the stretch before where they're one colour side by side.
    fn background(&self, row: &mut Row, col: u16, cells: u16) {
        let Some(color) = self.back else { return };
        match row.backgrounds.last_mut() {
            Some(last) if last.color == color && last.col + last.cells == col => last.cells += cells,
            _ => row.backgrounds.push(Background { col, cells, color }),
        }
    }

    fn glyph(&self, row: &mut Row, col: u16, text: GlyphText, wide: bool) {
        if self.shown {
            row.glyphs.push(Glyph { col, text, color: self.color, look: self.look, wide });
        }
    }
}

/// Draws each character of `span` from `col`, each `width` cells wide, and says the column after them.
fn chars(painted: &mut Row, span: &Span, cell: &Cell, mut col: u16, width: u16) -> u16 {
    for one in span.text.chars() {
        cell.background(painted, col, width);
        if one != ' ' || cell.lined() {
            cell.glyph(painted, col, GlyphText::Char(one), span.wide);
        }
        col += width;
    }
    col
}

fn rgb(palette: &[Rgb; 256], color: CellColor) -> Option<Rgb> {
    match color {
        CellColor::Palette(index) => Some(palette[usize::from(index)]),
        CellColor::Rgb { r, g, b } => Some(Rgb { r, g, b }),
        CellColor::Unknown => None,
    }
}
