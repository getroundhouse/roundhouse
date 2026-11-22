//! Each character's glyph, found once and kept, and painting one, or a cluster's.
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::{Font, FontId, GlyphId, Hsla, Pixels, SharedString, TextRun, Window, point};

/// Each character's glyph in each style, found once by shaping it alone, since GPUI keeps a shaped text only until it
/// next draws the window, and a row of fast output is new text every time. `None` for one no font has.
pub(crate) type Glyphs = Rc<RefCell<HashMap<GlyphKey, Option<FoundGlyph>>>>;

/// A character in a style, which is drawn with one glyph.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct GlyphKey {
    pub(super) character: char,
    pub(super) bold: bool,
    pub(super) italic: bool,
}

/// A character's glyph, in the font that has it.
#[derive(Clone, Copy)]
pub(crate) struct FoundGlyph {
    font: FontId,
    glyph: GlyphId,
    emoji: bool,
}

/// The glyph of `key`'s character in `font`, which is its style, from `glyphs`, or shaped and kept there the first
/// time it's asked for.
pub(super) fn find(
    glyphs: &Glyphs,
    key: GlyphKey,
    font: Font,
    ink: Hsla,
    font_size: Pixels,
    window: &Window,
) -> Option<FoundGlyph> {
    *glyphs.borrow_mut().entry(key).or_insert_with(|| {
        let shaped = shape(window, key.character.to_string(), font, ink, font_size);
        let run = shaped.runs.first()?;
        run.glyphs.first().map(|first| FoundGlyph { font: run.font_id, glyph: first.id, emoji: first.is_emoji })
    })
}

/// A cluster, which is rare enough to shape each time it's drawn, with its glyphs on the baseline at `origin`, as
/// every other cell's are.
pub(super) fn paint_cluster(
    window: &mut Window,
    origin: gpui_kit::Point<Pixels>,
    cluster: String,
    font: Font,
    font_size: Pixels,
    ink: Hsla,
) {
    let shaped = shape(window, cluster, font, ink, font_size);
    for run in &shaped.runs {
        for each in &run.glyphs {
            let at = point(origin.x + each.position.x, origin.y);
            let found = FoundGlyph { font: run.font_id, glyph: each.id, emoji: each.is_emoji };
            paint(window, at, found, font_size, ink);
        }
    }
}

/// One glyph, with its baseline at `origin`. One GPUI can't rasterize is left out, as a font without it would leave it.
pub(super) fn paint(
    window: &mut Window,
    origin: gpui_kit::Point<Pixels>,
    found: FoundGlyph,
    font_size: Pixels,
    ink: Hsla,
) {
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
