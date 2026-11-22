//! How big a cell is, and so how many fit in the room the grid has.
use gpui_kit::{Font, Pixels, Window};
use protocol::terminal::TerminalSize;

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
    pub(crate) fn fit(&self, bounds: gpui_kit::Size<Pixels>) -> TerminalSize {
        let cols = (bounds.width / self.cell_width).floor().max(1.);
        let rows = (bounds.height / self.line_height).floor().max(1.);
        TerminalSize { cols: whole_cells(cols), rows: whole_cells(rows) }
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

#[cfg(test)]
mod tests {
    use gpui_kit::{font, px, size};
    use protocol::terminal::TerminalSize;

    use super::Metrics;

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

        assert_eq!(metrics.fit(size(px(752.), px(330.))), TerminalSize { cols: 100, rows: 20 });
        assert_eq!(metrics.fit(size(px(0.), px(0.))), TerminalSize { cols: 1, rows: 1 });
    }
}
