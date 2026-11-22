//! How big a cell is, and so how many fit in the room the grid has.
use gpui_kit::{Font, Pixels, Window, px};
use protocol::terminal::TerminalSize;

/// How big a cell is, in the terminal's font.
#[derive(Clone, Debug)]
pub(crate) struct Metrics {
    pub(crate) font: Font,
    pub(crate) font_size: Pixels,
    /// A whole number of device pixels, at `scale`.
    pub(crate) cell_width: Pixels,
    pub(crate) line_height: Pixels,
    /// How far down a line its baseline is.
    pub(crate) baseline: Pixels,
    /// The window's device pixels to each of its pixels, which the cell was measured for.
    pub(crate) scale: f32,
}

/// How tall a cell is, for the size of the font: a value chosen to space lines about as a terminal app does.
const LINE_SPACING: f32 = 1.3;

impl Metrics {
    /// The cells of `font` at `font_size` in `window`, as wide as its `m` to the nearest device pixel, and as tall as
    /// its lines, in whole pixels.
    pub(crate) fn of(font: Font, font_size: Pixels, window: &Window) -> Self {
        let text = window.text_system();
        let id = text.resolve_font(&font);
        // A font has no `m` only if it has no letters, which no code font is, so this only gives the grid some width.
        let advance = text.advance(id, font_size, 'm').map_or(font_size * 0.6, |advance| advance.width);
        let scale = window.scale_factor();
        let line_height = (font_size * LINE_SPACING).round();
        let baseline = text.baseline_offset(id, font_size, line_height);
        Self { font, font_size, cell_width: whole_device_pixels(advance, scale), line_height, baseline, scale }
    }

    /// The same cells for `window`, measured again if its scale isn't the one they were measured for, as when the
    /// window moves to another display.
    pub(crate) fn for_window(&self, window: &Window) -> Option<Self> {
        let scale = window.scale_factor();
        #[expect(clippy::float_cmp, reason = "the scale is a display's, set rather than worked out, and only changes")]
        let same = scale == self.scale;
        (!same).then(|| Self::of(self.font.clone(), self.font_size, window))
    }

    /// How many cells fit in `bounds`, at least one each way.
    pub(crate) fn fit(&self, bounds: gpui_kit::Size<Pixels>) -> TerminalSize {
        let cols = (bounds.width / self.cell_width).floor().max(1.);
        let rows = (bounds.height / self.line_height).floor().max(1.);
        TerminalSize { cols: whole_cells(cols), rows: whole_cells(rows) }
    }
}

/// A cell's width for a font's `advance`, to the nearest whole device pixel at `scale`, and at least one. Every
/// column then starts a whole number of device pixels from the grid's left edge, so each cell's background, the selection and the cursor are as wide as every
/// other's, and a block glyph meets the one beside it, where a cell part of a pixel wide leaves a faint seam between
/// two `█`s on a 1x display. The text is then a little tighter or looser than the font spaces it. Rounded in device
/// pixels rather than the window's, as Ghostty does, so a scale such as 1.5 keeps the columns on them too.
fn whole_device_pixels(advance: Pixels, scale: f32) -> Pixels {
    px((f32::from(advance) * scale).round().max(1.) / scale)
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
    use gpui_kit::{TestAppContext, font, px, size};
    use protocol::terminal::TerminalSize;

    use super::{Metrics, whole_device_pixels};

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
            scale: 2.,
        };

        assert_eq!(metrics.fit(size(px(752.), px(330.))), TerminalSize { cols: 100, rows: 20 });
        assert_eq!(metrics.fit(size(px(0.), px(0.))), TerminalSize { cols: 1, rows: 1 });
    }

    /// Menlo's `m` at the code font's size is about 7.22 pixels wide, so at 1x and 2x a cell is 7, at 1.5x it's 11
    /// device pixels, and at 1.25x 9, and every column starts on a device pixel, however far along it is.
    #[test]
    fn a_cell_is_a_whole_number_of_device_pixels_wide() {
        let menlo = px(7.224_61);

        for (scale, device_pixels) in [(1., 7.), (2., 14.), (1.5, 11.), (1.25, 9.)] {
            let cell = whole_device_pixels(menlo, scale);

            assert!((f32::from(cell) * scale - device_pixels).abs() < 1e-4, "{cell:?} at {scale}x");
            let off =
                (0..600u16).map(|col| f32::from(cell) * f32::from(col) * scale).find(|x| (x - x.round()).abs() > 1e-2);
            assert_eq!(off, None, "every column on a device pixel at {scale}x");
        }
        assert_eq!(whole_device_pixels(px(0.2), 1.), px(1.), "at least one");
    }

    /// A window that moves to a display of another scale has its cells measured again, so they stay on its device
    /// pixels.
    #[gpui_kit::test]
    fn cells_are_measured_again_at_another_scale(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();

        cx.update(|window, _| {
            window.set_scale_factor(1.);
            let metrics = Metrics::of(font("Menlo"), px(12.), window);
            assert!(metrics.for_window(window).is_none(), "at the same scale");

            window.set_scale_factor(1.5);
            let again = metrics.for_window(window).unwrap();

            assert!((again.scale - 1.5).abs() < f32::EPSILON);
            let device_pixels = f32::from(again.cell_width) * 1.5;
            assert!((device_pixels - device_pixels.round()).abs() < 1e-4, "{:?} at 1.5x", again.cell_width);
        });
    }
}
