//! Every size of a bar and of a row, in one picture, as `look::BarSize` and `look::RowSize` have them: each by its name
//! and what it's the size of, as a raised band of its height, a bar's with its edge under it, holding words and an
//! outlined button of the size it holds, so the button's edges show the room above and below it, and how tall it is,
//! as the spec's tables write it.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::button::Button;
use gpui_kit::component::{IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    PlatformTextSystem, Render, Styled as _, TestSupportExt as _, Window, div, px, size,
};
use look::{BarSize, ControlSize, RowSize, Sizing as _, Type, Typeset as _};

use super::capture::Picture;
use super::headless;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "heights";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "look.md";
/// What the spec's tables call the types that name each size of a bar and of a row.
const BARS: &str = "look::BarSize";
const ROWS: &str = "look::RowSize";
const NAME_WIDTH: f32 = 110.;
const SAMPLE_WIDTH: f32 = 260.;
const WRITTEN_WIDTH: f32 = 60.;
/// Taller than the tallest, with room.
const ROW: f32 = 56.;
const MARGIN: f32 = 16.;

/// One band of the sheet: its name, as the spec writes it, with what it's the size of, how tall it is, written and
/// drawn, whether it has an edge under it, the kind of text it holds and the size of the button it holds.
struct Sample {
    name: String,
    /// What it's drawn with, so its place can be found.
    place: ElementId,
    tall: String,
    height: Pixels,
    edge: bool,
    kind: Type,
    button: ControlSize,
}

/// What the sheet shows, in its order: every size of a bar, then every size of a row.
fn samples() -> Vec<Sample> {
    let bars = BarSize::ALL.iter().map(|&bar| {
        let [name, tall] = bar.written();
        let (kind, button) = match bar {
            BarSize::Small => (Type::Detail, ControlSize::Small),
            BarSize::Regular => (Type::Body, ControlSize::Regular),
        };
        let place = places::id(BARS, &name);
        Sample { name: format!("{name} bar"), place, tall, height: bar.height(), edge: true, kind, button }
    });
    let rows = RowSize::ALL.iter().map(|&row| {
        let [name, tall] = row.written();
        let (kind, button) = (Type::Body, ControlSize::Small);
        let place = places::id(ROWS, &name);
        Sample { name: format!("{name} row"), place, tall, height: row.height(), edge: false, kind, button }
    });
    bars.chain(rows).collect()
}

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let row = |Sample { name, place, tall, height, edge, kind, button }: Sample| -> AnyElement {
            let outlined = Button::new(name.clone()).outline();
            let button = match button {
                ControlSize::Small => outlined.sized_square(button).icon(IconName::Plus),
                ControlSize::Regular | ControlSize::Large => outlined.sized(button).label("Add"),
            };
            let band = h_flex()
                .w(px(SAMPLE_WIDTH))
                .h(height)
                .pl_3()
                .pr_1()
                .bg(colors.raised)
                .when(edge, |band| band.border_b_1().border_color(colors.edge))
                .typeset(kind)
                .child(div().flex_1().child("What it says"))
                .child(button)
                .id(place)
                .test_support();
            let label = div().w(px(NAME_WIDTH)).typeset(Type::Detail).text_color(colors.text_muted).child(name);
            let written =
                div().w(px(WRITTEN_WIDTH)).pl_3().typeset(Type::Detail).text_color(colors.text_faint).child(tall);
            h_flex()
                .h(px(ROW))
                .border_b_1()
                .border_color(colors.edge)
                .child(label)
                .child(band)
                .child(written)
                .into_any_element()
        };
        v_flex()
            .size_full()
            .bg(colors.pane)
            .text_color(colors.text)
            .p(px(MARGIN))
            .children(samples().into_iter().map(row))
    }
}

/// At the app's scale, as the screens are, with where it draws each size's band.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    #[expect(clippy::cast_precision_loss, reason = "a handful of rows")]
    let count = samples().len() as f32;
    let width = px(MARGIN * 2. + NAME_WIDTH + SAMPLE_WIDTH + WRITTEN_WIDTH);
    let bars: Vec<String> = BarSize::ALL.iter().map(|bar| bar.written()[0].clone()).collect();
    let rows: Vec<String> = RowSize::ALL.iter().map(|row| row.written()[0].clone()).collect();
    let size = size(width, px(MARGIN * 2. + count * ROW));
    let measure = |window: &Window| Places::of(window, &[(BARS, &bars), (ROWS, &rows)]);
    headless::draw_and_measure(&mut cx, size, None, |_| Sheet, measure)
}
