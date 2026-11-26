//! Paints what's in a cell, or a run of them: a glyph, with its underline or line through it, and the cursor.
use client_core::drawn::{Cursor, CursorShape, Glyph, GlyphText, Rgb};
use gpui_kit::{BorderStyle, Bounds, Font, FontStyle, FontWeight, Pixels, Window, fill, outline, point, px, size};

use super::glyphs::{self, GlyphKey, Glyphs};
use super::metrics::Metrics;
use crate::color;

/// How thick a bar or underline cursor is.
const CURSOR_THICKNESS: Pixels = px(2.);

/// Paints cells at their place in the grid.
pub(super) struct Painter<'a> {
    pub(super) origin: gpui_kit::Point<Pixels>,
    pub(super) metrics: &'a Metrics,
    pub(super) glyphs: &'a Glyphs,
}

impl Painter<'_> {
    /// The box of `cells` cells from `col` along row `row`.
    pub(super) fn cells(&self, col: u16, row: u16, cells: u16) -> Bounds<Pixels> {
        let Metrics { cell_width, line_height, .. } = *self.metrics;
        let at = point(self.origin.x + cell_width * f32::from(col), self.origin.y + line_height * f32::from(row));
        Bounds::new(at, size(cell_width * f32::from(cells), line_height))
    }

    pub(super) fn glyph(&self, glyph: &Glyph, row: u16, ink: Rgb, window: &mut Window) {
        let cell = self.cells(glyph.col, row, if glyph.wide { 2 } else { 1 });
        let ink = if glyph.look.faint { color::faint(ink) } else { color::to_hsla(ink) };
        let Metrics { baseline, font_size, .. } = *self.metrics;
        if glyph.look.underline {
            let line = Bounds::new(point(cell.left(), cell.top() + baseline + px(1.)), size(cell.size.width, px(1.)));
            window.paint_quad(fill(line, ink));
        }
        if glyph.look.strikethrough {
            let middle = cell.top() + cell.size.height / 2.;
            window.paint_quad(fill(Bounds::new(point(cell.left(), middle), size(cell.size.width, px(1.))), ink));
        }
        let font = Font {
            weight: if glyph.look.bold { FontWeight::BOLD } else { FontWeight::NORMAL },
            style: if glyph.look.italic { FontStyle::Italic } else { FontStyle::Normal },
            ..self.metrics.font.clone()
        };
        let on_baseline = point(cell.left(), cell.top() + baseline);
        let one = match &glyph.text {
            GlyphText::Char(' ') => return,
            GlyphText::Char(one) => *one,
            GlyphText::Cluster(cluster) => {
                glyphs::paint_cluster(window, on_baseline, cluster.to_string(), font, font_size, ink);
                return;
            }
        };
        let key = GlyphKey { character: one, bold: glyph.look.bold, italic: glyph.look.italic };
        if let Some(found) = glyphs::find(self.glyphs, key, font, ink, font_size, window) {
            glyphs::paint(window, on_baseline, found, font_size, ink);
        }
    }

    /// A block over the cell while the view has the keys, and its outline while it doesn't, or a bar or a line under
    /// it in the shape the program set.
    pub(super) fn cursor(&self, at: Cursor, ink: Rgb, focused: bool, window: &mut Window) {
        let cell = self.cells(at.col, at.row, if at.wide { 2 } else { 1 });
        let ink = color::to_hsla(ink);
        let shape = match at.shape {
            CursorShape::Block if !focused => outline(cell, ink, BorderStyle::Solid),
            CursorShape::Block => fill(cell, ink),
            CursorShape::Bar => fill(Bounds::new(cell.origin, size(CURSOR_THICKNESS, cell.size.height)), ink),
            CursorShape::Underline => {
                let top = cell.bottom() - CURSOR_THICKNESS;
                fill(Bounds::new(point(cell.left(), top), size(cell.size.width, CURSOR_THICKNESS)), ink)
            }
        };
        window.paint_quad(shape);
    }
}
