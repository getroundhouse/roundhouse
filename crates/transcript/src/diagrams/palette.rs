use gpui_kit::{App, Global, Hsla};
use serde_json::{Map, Value, json};
use std::borrow::Cow;
use std::sync::Arc;

/// The size a diagram's words are drawn at, in its pixels, which is the chat's text's.
const FONT_SIZE: &str = "14px";

/// The palette made last, and the app's colours it was made from.
struct Kept {
    colors: look::Colors,
    palette: Palette,
}

impl Global for Kept {}

/// The colours a diagram is drawn in, from the app's, as Mermaid's base theme takes them, each by the name it gives it.
/// The theme works out the rest from these, such as each slice of a pie. Part of what a diagram's picture is kept by,
/// so a diagram is drawn again when the app's colours change.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Palette {
    colors: Arc<[(Cow<'static, str>, String)]>,
    /// Whether the app's ground is dark, which the theme works out lighter and darker shades by.
    dark: bool,
}

impl Palette {
    /// The app's colours, as a diagram's, made again only when they change, since each diagram drawn asks for them.
    pub(crate) fn of(cx: &mut App) -> Self {
        let colors = look::colors(cx);
        if let Some(kept) = cx.try_global::<Kept>().filter(|kept| kept.colors == *colors) {
            return kept.palette.clone();
        }
        let kept = Kept { colors: colors.clone(), palette: Self::from(colors) };
        let palette = kept.palette.clone();
        cx.set_global(kept);
        palette
    }

    pub(super) fn from(colors: &look::Colors) -> Self {
        let roles = parts(colors).into_iter().chain(charts(colors));
        let roles = roles.map(|(name, color)| (Cow::Borrowed(name), look::hex(color)));
        let scales = scales(colors.brand).map(|(name, color)| (Cow::Owned(name), look::hex(color)));
        Self { colors: roles.chain(scales).collect(), dark: colors.pane.l < 0.5 }
    }

    /// What `merman` is told to draw a diagram with: these colours, its words in `family`, as plain text rather than
    /// HTML, which only a browser draws.
    pub(super) fn config(&self, family: &str) -> Value {
        let mut variables: Map<String, Value> =
            self.colors.iter().map(|(name, color)| (name.to_string(), Value::from(color.as_str()))).collect();
        variables.insert("darkMode".into(), Value::Bool(self.dark));
        variables.insert("fontFamily".into(), family.into());
        variables.insert("fontSize".into(), FONT_SIZE.into());
        // The dark theme's is a pale glow round each box, which the app draws nothing like.
        variables.insert("dropShadow".into(), "none".into());
        json!({
            "theme": "base",
            "darkMode": self.dark,
            "fontFamily": family,
            "fontSize": FONT_SIZE,
            "htmlLabels": false,
            "flowchart": { "htmlLabels": false },
            "themeVariables": variables,
        })
    }
}

/// The colours of a diagram's boxes, lines and words, by the roles of the app's they take.
fn parts(colors: &look::Colors) -> [(&'static str, Hsla); 35] {
    [
        ("background", colors.pane),
        ("primaryColor", colors.raised),
        ("primaryTextColor", colors.text),
        ("primaryBorderColor", colors.control_edge),
        ("secondaryColor", colors.inset),
        ("secondaryTextColor", colors.text),
        ("secondaryBorderColor", colors.control_edge),
        ("tertiaryColor", colors.sidebar),
        ("tertiaryTextColor", colors.text),
        ("tertiaryBorderColor", colors.edge),
        ("mainBkg", colors.raised),
        ("nodeBorder", colors.control_edge),
        ("clusterBkg", colors.inset),
        ("clusterBorder", colors.edge),
        ("lineColor", colors.text_muted),
        ("textColor", colors.text),
        ("titleColor", colors.text),
        ("edgeLabelBackground", colors.pane),
        ("noteBkgColor", colors.prompt),
        ("noteTextColor", colors.text),
        ("noteBorderColor", colors.control_edge),
        ("actorBkg", colors.raised),
        ("actorBorder", colors.control_edge),
        ("actorTextColor", colors.text),
        ("actorLineColor", colors.text_faint),
        ("signalColor", colors.text_muted),
        ("signalTextColor", colors.text),
        ("labelBoxBkgColor", colors.raised),
        ("labelBoxBorderColor", colors.control_edge),
        ("labelTextColor", colors.text),
        ("loopTextColor", colors.text),
        ("rowOdd", colors.raised),
        ("rowEven", colors.inset),
        ("attributeBackgroundColorOdd", colors.raised),
        ("attributeBackgroundColorEven", colors.inset),
    ]
}

/// The colours of the parts of charts, such as a Gantt chart's sections and tasks and a pie's edges.
fn charts(colors: &look::Colors) -> [(&'static str, Hsla); 28] {
    [
        ("sectionBkgColor", colors.inset),
        ("altSectionBkgColor", colors.pane),
        ("sectionBkgColor2", colors.sidebar),
        ("gridColor", colors.edge),
        ("todayLineColor", colors.brand),
        ("taskBkgColor", colors.raised),
        ("taskBorderColor", colors.control_edge),
        ("taskTextColor", colors.text),
        ("taskTextLightColor", colors.text),
        ("taskTextDarkColor", colors.text),
        ("taskTextOutsideColor", colors.text),
        ("taskTextClickableColor", colors.link),
        ("activeTaskBkgColor", colors.prompt),
        ("activeTaskBorderColor", colors.brand),
        ("doneTaskBkgColor", colors.inset),
        ("doneTaskBorderColor", colors.edge),
        ("critBkgColor", colors.raised),
        ("critBorderColor", colors.error),
        ("pieStrokeColor", colors.pane),
        ("pieOuterStrokeColor", colors.edge),
        ("pieTitleTextColor", colors.text),
        ("pieSectionTextColor", colors.text),
        ("pieLegendTextColor", colors.text),
        ("commitLabelBackground", colors.raised),
        ("commitLabelColor", colors.text),
        ("tagLabelBackground", colors.raised),
        ("tagLabelColor", colors.text),
        ("tagLabelBorder", colors.control_edge),
    ]
}

/// How many colours the kinds of diagram that colour their parts in turn have, such as a pie's slices, a timeline's
/// sections and a git graph's branches: Mermaid's most.
const IN_TURN: u16 = 12;

/// The colours the kinds of diagram that colour their parts in turn take, as Mermaid's theme names them, each by its
/// place in the turn: hues spread round from `brand`'s, each far from the one before, quiet enough for the app's light
/// text over them, as fills, and brighter as lines and edges.
fn scales(brand: Hsla) -> impl Iterator<Item = (String, Hsla)> {
    (0..IN_TURN).flat_map(move |place| {
        let hue = (brand.h + f32::from(place * 150 % 360) / 360.).fract();
        let shade = |s: f32, l: f32| Hsla { h: hue, s, l, a: 1. };
        let label = Hsla { h: hue, s: 0.2, l: 0.92, a: 1. };
        let fill = shade(0.35, 0.3);
        let mut named = vec![
            (format!("cScale{place}"), fill),
            (format!("cScalePeer{place}"), shade(0.45, 0.45)),
            (format!("cScaleLabel{place}"), label),
            (format!("pie{}", place + 1), shade(0.4, 0.4)),
        ];
        if place < 8 {
            named.push((format!("git{place}"), shade(0.45, 0.5)));
            named.push((format!("gitBranchLabel{place}"), label));
        }
        named
    })
}
