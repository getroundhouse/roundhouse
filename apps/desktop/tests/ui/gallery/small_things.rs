//! The small things that sit in a line, each kind in one picture of its own: every kind of pill, every kind of badge
//! beside what it's said of, with labels in four colours, and every kind of count beside what it counts, with the
//! lines a file gained and lost, as `look` has them, at the app's scale.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem,
    Render, Styled as _, TestSupportExt as _, Window, div, px, rgb, size,
};
use look::{BadgeKind, CountKind, PillKind, Type, Typeset as _};

use super::capture::Picture;
use super::headless;
use super::places::{self, Places};

/// The images' names, in `docs/spec/screens/`.
pub(crate) const PILLS: &str = "pill-kinds";
pub(crate) const BADGES: &str = "badge-kinds";
pub(crate) const COUNTS: &str = "count-kinds";
/// The spec's file that shows them.
pub(crate) const SPEC: &str = "components.md";
const NAME_WIDTH: f32 = 110.;
const SAMPLE_WIDTH: f32 = 360.;
const ROW: f32 = 44.;
const MARGIN: f32 = 16.;

/// A row of a sheet: the name of what it shows, faint, then what it shows, beside the words it's said with.
fn row(name: String, sample: AnyElement, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    h_flex()
        .h(px(ROW))
        .border_b_1()
        .border_color(colors.edge)
        .child(div().w(px(NAME_WIDTH)).flex_none().typeset(Type::Detail).text_color(colors.text_muted).child(name))
        .child(div().w(px(SAMPLE_WIDTH)).child(sample))
        .into_any_element()
}

/// `words`, then `sample`, in a line, as it's drawn in the app, with the sample's place found by `kind` and `name`.
fn in_a_line(kind: &str, name: &str, words: &'static str, sample: impl IntoElement, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    h_flex()
        .gap(look::Space::Small)
        .typeset(Type::Body)
        .child(div().flex_none().text_color(colors.text_muted).child(words))
        .child(div().id(places::id(kind, name)).flex().child(sample).test_support())
        .into_any_element()
}

struct Pills;

impl Render for Pills {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let rows: Vec<AnyElement> = PillKind::ALL
            .iter()
            .map(|&kind| {
                let [name] = kind.written();
                let (words, value) = match kind {
                    PillKind::Plain => ("Ran", "cargo test --workspace"),
                    PillKind::Failure => ("Failed", "error: could not compile `look`"),
                };
                let sample = in_a_line("look::PillKind", &name, words, look::pill(kind, value), cx);
                row(name, sample, cx)
            })
            .collect();
        v_flex().size_full().bg(colors.pane).text_color(colors.text).p(px(MARGIN)).children(rows)
    }
}

/// The labels the sheet draws, in GitHub's colours: a pale one, a bright one, a cool one and a dark one.
const LABELS: [(&str, u32); 4] = [
    ("needs/owner", 0x00fb_ca04),
    ("kind/question", 0x00d8_76e3),
    ("area/sidebar", 0x00c5_def5),
    ("blocked", 0x00b6_0205),
];

struct Badges;

impl Render for Badges {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let mut rows: Vec<AnyElement> = BadgeKind::ALL
            .iter()
            .map(|&kind| {
                let [name] = kind.written();
                let (words, badge) = match kind {
                    BadgeKind::Plain => ("Work account", "Default"),
                    BadgeKind::Warning => ("Personal account", "Ends in 3 days"),
                    BadgeKind::Failure => ("The request to the model failed", "Twice"),
                };
                let sample = in_a_line("look::BadgeKind", &name, words, look::badge(kind, badge), cx);
                row(name, sample, cx)
            })
            .collect();
        let labels = h_flex()
            .gap(look::Space::XSmall)
            .children(LABELS.map(|(words, color)| look::label(words, Hsla::from(rgb(color)))));
        rows.push(row("Labels".to_string(), labels.into_any_element(), cx));
        v_flex().size_full().bg(colors.pane).text_color(colors.text).p(px(MARGIN)).children(rows)
    }
}

struct Counts;

impl Render for Counts {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let line_text = look::LineText::new(window, cx);
        let mut rows: Vec<AnyElement> = CountKind::ALL
            .iter()
            .map(|&kind| {
                let [name] = kind.written();
                let (words, how_many) = match kind {
                    CountKind::NeedsALook => ("signal-box", 3),
                    CountKind::HowMany => ("Changes", 12),
                };
                let sample = in_a_line("look::CountKind", &name, words, look::count(kind, how_many), cx);
                row(name, sample, cx)
            })
            .collect();
        let lines = h_flex().gap(look::Space::Large).typeset(Type::Body).text_color(colors.text_muted).children(
            [(12, 3, "model.rs"), (40, 0, "Cargo.toml"), (0, 120, "legacy.rs")].map(|(added, removed, file)| {
                h_flex()
                    .gap(look::Space::Small)
                    .child(line_text.body.set(div().child(file)))
                    .child(look::lines_changed(added, removed).placed(&line_text.code))
            }),
        );
        rows.push(row("Lines".to_string(), lines.into_any_element(), cx));
        v_flex().size_full().bg(colors.pane).text_color(colors.text).p(px(MARGIN)).children(rows)
    }
}

#[expect(clippy::cast_precision_loss, reason = "a handful of rows")]
fn tall(rows: usize) -> gpui_kit::Pixels {
    px(MARGIN * 2. + rows as f32 * ROW)
}

fn wide() -> gpui_kit::Pixels {
    px(MARGIN * 2. + NAME_WIDTH + SAMPLE_WIDTH)
}

/// Every kind of pill, with where it draws each.
pub(crate) fn pills(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    let names: Vec<String> = PillKind::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let size = size(wide(), tall(PillKind::ALL.len()));
    headless::draw_and_measure(
        &mut cx,
        size,
        None,
        |_| Pills,
        |window| Places::of(window, &[("look::PillKind", &names)]),
    )
}

/// Every kind of badge, and labels, with where it draws each badge.
pub(crate) fn badges(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    let names: Vec<String> = BadgeKind::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let size = size(wide(), tall(BadgeKind::ALL.len() + 1));
    headless::draw_and_measure(
        &mut cx,
        size,
        None,
        |_| Badges,
        |window| Places::of(window, &[("look::BadgeKind", &names)]),
    )
}

/// Every kind of count, and lines changed, with where it draws each count.
pub(crate) fn counts(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    let names: Vec<String> = CountKind::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let size = size(wide(), tall(CountKind::ALL.len() + 1));
    headless::draw_and_measure(
        &mut cx,
        size,
        None,
        |_| Counts,
        |window| Places::of(window, &[("look::CountKind", &names)]),
    )
}
