//! Text of different sizes or fonts along one row, on one baseline, as type is set.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{App, Pixels, ScaledPixels, SharedString, StyleRefinement, Styled, TextRun, Window, font, phi, px};

/// Each kind of text a line can mix, placed on body text's baseline. A row centres what's in it, and a line's baseline
/// is below the middle of its box by half of how far its font reaches above the baseline less how far below, so
/// smaller text, or text in a font that reaches less far up, would sit higher than body text beside it. A row that
/// lines its text up along the top, rather than centring it, puts it on the baseline the same way, since each line's
/// box then starts at the same place.
///
/// Every line is spaced as GPUI spaces body text by default, so each line's box is as tall as the others, and body
/// text that had that spacing stays where it was.
#[derive(Clone, Debug)]
pub struct LineText {
    /// The system font at the kit's small size, which body text is.
    pub body: Placed,
    /// The system font at the kit's extra-small size, for small details.
    pub small: Placed,
    /// The code font at its own size.
    pub code: Placed,
}

impl LineText {
    /// Worked out from the window once for each frame, and handed to everything on it that mixes text.
    pub fn new(window: &Window, cx: &App) -> Self {
        let theme = cx.theme();
        let rem = window.rem_size();
        let (sans, mono) = (theme.font_family.clone(), theme.mono_font_family.clone());
        let body_size = kit_size(Styled::text_sm, rem);
        // Rounded to whole device pixels, as GPUI rounds a line's height before it lays the line out and draws it.
        let line_height = window.pixel_snap(phi().to_pixels(body_size.into(), rem));
        let body = Placing { line_height, below_middle: below_middle(&sans, body_size, window) };
        Self {
            small: body.place(sans.clone(), kit_size(Styled::text_xs, rem), window),
            code: body.place(mono, theme.mono_font_size, window),
            body: body.place(sans, body_size, window),
        }
    }
}

/// Text placed on a baseline: its font, its size, its line, and how far it moves to sit there.
#[derive(Clone, Debug)]
pub struct Placed {
    family: SharedString,
    size: Pixels,
    line_height: Pixels,
    shift: Pixels,
}

impl Placed {
    /// `element` in this text, on its baseline. Its weight is left to it, since a family's weights reach as far above
    /// and below the baseline as each other.
    pub fn set<E: Styled>(&self, element: E) -> E {
        element.font_family(self.family.clone()).text_size(self.size).line_height(self.line_height).top(self.shift)
    }
}

/// Body text's line, which the rest is placed against.
#[derive(Clone, Copy)]
struct Placing {
    line_height: Pixels,
    below_middle: Pixels,
}

impl Placing {
    fn place(self, family: SharedString, size: Pixels, window: &Window) -> Placed {
        // GPUI rounds the move to whole device pixels, and then each glyph's baseline to the nearest one, so the two
        // roundings could leave the baselines a pixel apart. The move is worked out instead from the pixel each
        // baseline lands on, down from the top of its line's box, which GPUI also puts on a whole pixel.
        let scale = window.scale_factor();
        let lands = |below_middle: Pixels| nearest_pixel((self.line_height / 2. + below_middle).scale(scale));
        let shift = lands(self.below_middle) - lands(below_middle(&family, size, window));
        Placed { family, size, line_height: self.line_height, shift: px(shift.0 / scale) }
    }
}

/// The size one of the kit's text sizes, such as `text_sm`, sets.
fn kit_size(size: impl FnOnce(StyleRefinement) -> StyleRefinement, rem: Pixels) -> Pixels {
    size(StyleRefinement::default()).text.font_size.map_or(rem, |size| size.to_pixels(rem))
}

/// How far a line's baseline is below the middle of its box: half of what's left of the line once its ascent and
/// descent are taken out goes above them. They're read from a line laid out in the font, as the text system lays out
/// the line it draws, rather than from the font's own figures, which a platform's text system can add to.
fn below_middle(family: &SharedString, size: Pixels, window: &Window) -> Pixels {
    const SAMPLE: &str = "Hg";
    let run = TextRun { len: SAMPLE.len(), font: font(family.clone()), ..TextRun::default() };
    let line = window.text_system().layout_line(SAMPLE, size, &[run], None);
    (line.ascent - line.descent) / 2.
}

/// The nearest whole device pixel, and the one above at a tie, as GPUI places a glyph.
fn nearest_pixel(at: ScaledPixels) -> ScaledPixels {
    ScaledPixels((at.0 - 0.5).ceil())
}
