//! Paints a terminal's frame as a grid of cells, inside one layer, with each cell's character painted as a glyph at its
//! column, as `docs/performance.md` says makes a terminal fast to draw. It also says where on it the pointer is pressed
//! and dragged, in cells, which selects.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use client_core::OpenTerminal;
use gpui_kit::{
    App, BorderStyle, Bounds, CursorStyle, DispatchPhase, Element, ElementId, Font, FontId, FontStyle, FontWeight,
    GlobalElementId, GlyphId, Hitbox, HitboxBehavior, Hsla, InspectorElementId, IntoElement, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, SharedString, Style, TextRun, Window, fill, outline, point,
    px, relative, rgb, size,
};
use terminal_state::drawn::{Cursor, CursorShape, Frame, Glyph, GlyphText, Rgb};
use terminal_state::{Pointer, SelectBy, Size};

/// How big a cell is, in the terminal's font.
#[derive(Clone, Debug)]
pub(crate) struct Metrics {
    pub(crate) font: Font,
    pub(crate) font_size: Pixels,
    pub(crate) cell_width: Pixels,
    pub(crate) line_height: Pixels,
    /// How far down a line its baseline is.
    pub(crate) baseline: Pixels,
}

/// How tall a cell is, for the size of the font: a value chosen to space lines about as a terminal app does.
const LINE_SPACING: f32 = 1.3;
/// How thick a bar or underline cursor is.
const CURSOR_THICKNESS: Pixels = px(2.);

impl Metrics {
    /// The cells of `font` at `font_size`, as wide as its `m`, and as tall as its lines, in whole pixels, though a cell
    /// can be part of a pixel wide.
    pub(crate) fn of(font: Font, font_size: Pixels, window: &Window) -> Self {
        let text = window.text_system();
        let id = text.resolve_font(&font);
        // A font has no `m` only if it has no letters, which no code font is, so this only gives the grid some width.
        let cell_width = text.advance(id, font_size, 'm').map_or(font_size * 0.6, |advance| advance.width);
        let line_height = (font_size * LINE_SPACING).round();
        let baseline = text.baseline_offset(id, font_size, line_height);
        Self { font, font_size, cell_width, line_height, baseline }
    }

    /// How many cells fit in `bounds`, at least one each way.
    pub(crate) fn fit(&self, bounds: gpui_kit::Size<Pixels>) -> Size {
        let cols = (bounds.width / self.cell_width).floor().max(1.);
        let rows = (bounds.height / self.line_height).floor().max(1.);
        Size { cols: whole_cells(cols), rows: whole_cells(rows) }
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a count of cells, at least one, that fits"
)]
fn whole_cells(count: f32) -> u16 {
    count.min(f32::from(u16::MAX)) as u16
}

/// Each character's glyph in each style, found once by shaping it alone, since GPUI keeps a shaped text only until it
/// next draws the window, and a row of fast output is new text every time. `None` for one no font has.
pub(crate) type Glyphs = Rc<RefCell<HashMap<GlyphKey, Option<FoundGlyph>>>>;

/// A character in a style, which is drawn with one glyph.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct GlyphKey {
    character: char,
    bold: bool,
    italic: bool,
}

/// A character's glyph, in the font that has it.
#[derive(Clone, Copy)]
pub(crate) struct FoundGlyph {
    font: FontId,
    glyph: GlyphId,
    emoji: bool,
}

/// The terminal's grid, which fills its parent.
pub(crate) struct Grid {
    pub(crate) frame: Option<Arc<Frame>>,
    pub(crate) background: Rgb,
    pub(crate) metrics: Metrics,
    pub(crate) glyphs: Glyphs,
    /// Whether the view has the keys, which a block cursor shows by being filled.
    pub(crate) focused: bool,
    /// What selected cells are painted in, under their text.
    pub(crate) selection: Hsla,
    /// Told how many cells fit, once the grid is laid out.
    pub(crate) fits: Box<dyn Fn(Size)>,
    /// Where presses and drags on the grid go.
    pub(crate) pointing: Pointing,
}

/// Where a press on the grid, and the drag that follows it, are told.
#[derive(Clone)]
pub(crate) struct Pointing {
    pub(crate) open: OpenTerminal,
    /// Whether the button went down on the grid and hasn't come up, which a drag past its edges goes on from.
    pub(crate) dragging: Rc<Cell<bool>>,
}

impl IntoElement for Grid {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Grid {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        window: &mut Window,
        _: &mut App,
    ) -> Hitbox {
        (self.fits)(self.metrics.fit(bounds.size));
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        hitbox: &mut Hitbox,
        window: &mut Window,
        _: &mut App,
    ) {
        window.set_cursor_style(CursorStyle::IBeam, hitbox);
        follow_the_pointer(self.pointing.clone(), hitbox.clone(), bounds.origin, &self.metrics, window);
        // One layer for the grid, so GPUI orders it once rather than searching what each box and glyph is over. Within
        // a layer, boxes are drawn before glyphs, so a background or the cursor is always under the text.
        window.paint_layer(bounds, |window| {
            // The terminal's own, which a program can change, or swap with the text's, once there's a terminal's frame.
            let background = self.frame.as_ref().map_or(self.background, |frame| frame.background);
            window.paint_quad(fill(bounds, color(background)));
            let Some(frame) = self.frame.clone() else { return };
            let painter = Painter { origin: bounds.origin, metrics: &self.metrics, glyphs: &self.glyphs };
            let cursor = frame.cursor.filter(|_| self.focused);
            for (row, cells) in (0u16..).zip(&frame.rows) {
                for back in &cells.backgrounds {
                    window.paint_quad(fill(painter.cells(back.col, row, back.cells), color(back.color)));
                }
                if let Some(selected) = cells.selected {
                    window.paint_quad(fill(painter.cells(selected.col, row, selected.cells), self.selection));
                }
                for glyph in &cells.glyphs {
                    let under_cursor =
                        cursor.is_some_and(|at| at.shape == CursorShape::Block && at.row == row && at.col == glyph.col);
                    let ink = if under_cursor { frame.background } else { glyph.color };
                    painter.glyph(glyph, row, ink, window);
                }
            }
            if let Some(at) = frame.cursor {
                painter.cursor(at, at.color.unwrap_or(frame.foreground), self.focused, window);
            }
        });
    }
}

/// Gives client-core's copy of the terminal a left press on the grid, where the pointer goes with the button down after
/// it, even past the grid's edges, and where the button comes up, each in cells from the grid's top left.
fn follow_the_pointer(
    pointing: Pointing,
    hitbox: Hitbox,
    origin: gpui_kit::Point<Pixels>,
    metrics: &Metrics,
    window: &mut Window,
) {
    let (cell_width, line_height) = (metrics.cell_width, metrics.line_height);
    let at = move |position: gpui_kit::Point<Pixels>| Pointer {
        col: (position.x - origin.x) / cell_width,
        row: (position.y - origin.y) / line_height,
    };
    let (down, moved, up) = (pointing.clone(), pointing.clone(), pointing);
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _| {
        if phase == DispatchPhase::Bubble && event.button == MouseButton::Left && hitbox.is_hovered(window) {
            down.dragging.set(true);
            down.open.press(at(event.position), select_by(event.click_count));
        }
    });
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, _| {
        if phase == DispatchPhase::Bubble && moved.dragging.get() {
            if event.pressed_button == Some(MouseButton::Left) {
                moved.open.drag(at(event.position));
            } else {
                // The button came up where the window didn't hear it, such as over another app.
                moved.dragging.set(false);
                moved.open.release(at(event.position));
            }
        }
    });
    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, _| {
        if phase == DispatchPhase::Bubble && event.button == MouseButton::Left && up.dragging.replace(false) {
            up.open.release(at(event.position));
        }
    });
}

/// What the `clicks`th press of a run selects by, as the platform counts them. Past the third, it's still the line.
fn select_by(clicks: usize) -> SelectBy {
    match clicks {
        0 | 1 => SelectBy::Cells,
        2 => SelectBy::Words,
        _ => SelectBy::Lines,
    }
}

/// Paints cells at their place in the grid.
struct Painter<'a> {
    origin: gpui_kit::Point<Pixels>,
    metrics: &'a Metrics,
    glyphs: &'a Glyphs,
}

impl Painter<'_> {
    /// The box of `cells` cells from `col` along row `row`.
    fn cells(&self, col: u16, row: u16, cells: u16) -> Bounds<Pixels> {
        let Metrics { cell_width, line_height, .. } = *self.metrics;
        let at = point(self.origin.x + cell_width * f32::from(col), self.origin.y + line_height * f32::from(row));
        Bounds::new(at, size(cell_width * f32::from(cells), line_height))
    }

    fn glyph(&self, glyph: &Glyph, row: u16, ink: Rgb, window: &mut Window) {
        let cell = self.cells(glyph.col, row, if glyph.wide { 2 } else { 1 });
        let mut ink = color(ink);
        if glyph.look.faint {
            ink = ink.opacity(0.5);
        }
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
        let one = match &glyph.text {
            GlyphText::Char(' ') => return,
            GlyphText::Char(one) => *one,
            // A cluster is rare enough to shape each time it's drawn, and its glyphs go on the cell's baseline, as
            // every other cell's do.
            GlyphText::Cluster(cluster) => {
                let shaped = shape(window, cluster.to_string(), font, ink, font_size);
                for run in &shaped.runs {
                    for each in &run.glyphs {
                        let origin = point(cell.left() + each.position.x, cell.top() + baseline);
                        let found = FoundGlyph { font: run.font_id, glyph: each.id, emoji: each.is_emoji };
                        paint(window, origin, found, font_size, ink);
                    }
                }
                return;
            }
        };
        let key = GlyphKey { character: one, bold: glyph.look.bold, italic: glyph.look.italic };
        let found = *self.glyphs.borrow_mut().entry(key).or_insert_with(|| {
            let shaped = shape(window, one.to_string(), font, ink, font_size);
            let run = shaped.runs.first()?;
            run.glyphs.first().map(|first| FoundGlyph { font: run.font_id, glyph: first.id, emoji: first.is_emoji })
        });
        if let Some(found) = found {
            paint(window, point(cell.left(), cell.top() + baseline), found, font_size, ink);
        }
    }

    /// A block over the cell while the view has the keys, and its outline while it doesn't, or a bar or a line under
    /// it in the shape the program set.
    fn cursor(&self, at: Cursor, ink: Rgb, focused: bool, window: &mut Window) {
        let cell = self.cells(at.col, at.row, if at.wide { 2 } else { 1 });
        let ink = color(ink);
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

/// One glyph, with its baseline at `origin`. One GPUI can't rasterize is left out, as a font without it would leave it.
fn paint(window: &mut Window, origin: gpui_kit::Point<Pixels>, found: FoundGlyph, font_size: Pixels, ink: Hsla) {
    let _ = if found.emoji {
        window.paint_emoji(origin, found.font, found.glyph, font_size)
    } else {
        window.paint_glyph(origin, found.font, found.glyph, font_size, ink)
    };
}

fn shape(window: &Window, text: String, font: Font, ink: Hsla, font_size: Pixels) -> gpui_kit::ShapedLine {
    let run =
        TextRun { len: text.len(), font, color: ink, background_color: None, underline: None, strikethrough: None };
    window.text_system().shape_line(SharedString::from(text), font_size, &[run], None)
}

fn color(rgb_color: Rgb) -> Hsla {
    rgb(rgb_color.hex()).into()
}

#[cfg(test)]
mod tests {
    use gpui_kit::{font, px, size};
    use terminal_state::{SelectBy, Size};

    use super::{Metrics, select_by};

    /// Only whole cells, so the terminal is never bigger than what shows, and one each way however little room there
    /// is.
    #[test]
    fn as_many_whole_cells_fit_as_there_is_room_for() {
        let metrics = Metrics {
            font: font("Menlo"),
            font_size: px(13.),
            cell_width: px(7.5),
            line_height: px(16.),
            baseline: px(12.),
        };

        assert_eq!(metrics.fit(size(px(752.), px(330.))), Size { cols: 100, rows: 20 });
        assert_eq!(metrics.fit(size(px(0.), px(0.))), Size { cols: 1, rows: 1 });
    }

    /// A double click selects a word, and a triple click the line, as the platform counts them, which is 0 for a press
    /// with no count.
    #[test]
    fn each_click_of_a_run_selects_by_more() {
        assert_eq!(
            [0, 1, 2, 3, 4].map(select_by),
            [SelectBy::Cells, SelectBy::Cells, SelectBy::Words, SelectBy::Lines, SelectBy::Lines]
        );
    }
}
