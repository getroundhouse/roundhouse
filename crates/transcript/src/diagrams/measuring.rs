//! How a diagram's words are measured: with the font they'll be drawn in, which is found once, among the machine's
//! fonts, as GPUI finds the font an SVG names, and which the diagram names by its family. So each box is as wide as its
//! words are drawn. Without it, `merman` measures words by a font of its own, and words drawn in another come out wider
//! than their box, and are cut off.

use crate::linked_images::system_fonts;
use merman::SvgEnvironment;
use merman::svg::{
    DeterministicTextMeasurer, MeasurementProfileId, TextMeasurementPolicy, TextMeasurementProfile,
    TextMeasurementProfileIdentity, TextStyle,
};
use rustybuzz::Face;
use std::sync::LazyLock;
use usvg::fontdb::{Family, Query, Weight};

/// The families a diagram's words are drawn in, the first the machine has: plain ones each platform has one of. One
/// without any of them draws them in its own sans serif.
const FAMILIES: [&str; 6] = ["Helvetica Neue", "Helvetica", "Arial", "Segoe UI", "Noto Sans", "DejaVu Sans"];

/// The font a diagram's words are drawn in, regular and bold, ready to shape words with.
pub(super) struct Fonts {
    /// What the diagram names it by, which finds the same font when GPUI draws it.
    pub(super) family: String,
    regular: Face<'static>,
    bold: Face<'static>,
}

/// The font a diagram's words are drawn in, found the first time one's drawn, among the fonts GPUI finds an SVG's in,
/// or `None` on a machine with no font, which can't draw a diagram's words.
pub(super) fn fonts() -> Option<&'static Fonts> {
    static FONTS: LazyLock<Option<Fonts>> = LazyLock::new(find);
    FONTS.as_ref()
}

fn find() -> Option<Fonts> {
    let fonts = system_fonts();
    let face = |family: Family<'_>, weight| {
        let id = fonts.query(&Query { families: &[family], weight, ..Query::default() })?;
        Some((id, fonts.face(id)?.families.first()?.0.clone()))
    };
    let named = FAMILIES.iter().map(|name| Family::Name(name));
    let (regular, family) = named.chain([Family::SansSerif]).find_map(|family| face(family, Weight::NORMAL))?;
    let bold = face(Family::Name(&family), Weight::BOLD).map_or(regular, |(bold, _)| bold);
    // Kept for as long as the app runs, as the faces read from them are.
    let shaped = |id| {
        let (bytes, index) = fonts.with_face_data(id, |bytes, index| (bytes.to_vec(), index))?;
        Face::from_slice(Box::leak(bytes.into_boxed_slice()), index)
    };
    let regular_face = shaped(regular)?;
    let bold =
        if bold == regular { regular_face.clone() } else { shaped(bold).unwrap_or_else(|| regular_face.clone()) };
    Some(Fonts { family, regular: regular_face, bold })
}

/// What `merman` draws with, which measures words with `fonts`.
pub(super) fn environment(fonts: &'static Fonts) -> SvgEnvironment {
    let measurer =
        DeterministicTextMeasurer::default().with_width_callback(move |text, style| fonts.width(text, style));
    let known = MeasurementProfileId::new("roundhouse.system")
        .ok()
        .and_then(|id| TextMeasurementProfileIdentity::new(id, format!("1:{}", fonts.family)).ok());
    let Some(known) = known else { return SvgEnvironment::deterministic() };
    let policy = TextMeasurementPolicy::uniform(TextMeasurementProfile::new(known, measurer));
    SvgEnvironment::deterministic().with_text_measurement_policy(policy)
}

impl Fonts {
    /// How wide `text` is in `style`, in the diagram's pixels.
    fn width(&self, text: &str, style: &TextStyle) -> f64 {
        let bold = matches!(style.font_weight.as_deref(), Some("bold" | "bolder" | "600" | "700" | "800" | "900"));
        let face = if bold { &self.bold } else { &self.regular };
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        let shaped = rustybuzz::shape(face, &[], buffer);
        let advance: i64 = shaped.glyph_positions().iter().map(|glyph| i64::from(glyph.x_advance)).sum();
        #[expect(clippy::cast_precision_loss, reason = "a line's width, in a font's units, is far below 2^52")]
        let advance = advance as f64;
        advance / f64::from(face.units_per_em()) * style.font_size.max(1.)
    }
}
