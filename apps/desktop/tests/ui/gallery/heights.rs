//! Every size of a bar and of a row, in one picture, as `look::BarSize` and `look::RowSize` have them: each by its name
//! and what it's the size of, as a raised band of its height, a bar's with its edge under it, holding words and an
//! outlined button of the size it holds, so the button's edges show the room above and below it, and how tall it is,
//! as the spec's tables write it.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{IconName, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, ParentElement as _, Pixels, PlatformTextSystem, Styled as _,
    TestSupportExt as _, div, px,
};
use look::{BarSize, ButtonKind, ControlSize, Lined as _, RowSize, Space, Type, Typeset as _};

use super::capture::Picture;
use super::foundation_sheet::Columns;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "heights";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "look.md";
/// What the spec's tables call the types that name each size of a bar and of a row.
const BARS: &str = "look::BarSize";
const ROWS: &str = "look::RowSize";
const BAND_WIDTH: f32 = 260.;
/// Its samples a band and the room after it, and its rows taller than the tallest, with room.
const COLUMNS: Columns = Columns { name: 110., sample: BAND_WIDTH + 12., written: 48., row: 56. };

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
        let [name, tall, _] = bar.written();
        let (kind, button) = match bar {
            BarSize::Small => (Type::Detail, ControlSize::Small),
            BarSize::Regular => (Type::Body, ControlSize::Regular),
        };
        let place = places::id(BARS, &name);
        Sample { name: format!("{name} bar"), place, tall, height: bar.height(), edge: true, kind, button }
    });
    let rows = RowSize::ALL.iter().map(|&row| {
        let [name, tall, _] = row.written();
        let (kind, button) = (Type::Body, ControlSize::Small);
        let place = places::id(ROWS, &name);
        Sample { name: format!("{name} row"), place, tall, height: row.height(), edge: false, kind, button }
    });
    bars.chain(rows).collect()
}

fn rows(cx: &App) -> Vec<AnyElement> {
    let colors = look::colors(cx);
    let row = |Sample { name, place, tall, height, edge, kind, button }: Sample| {
        let button = match button {
            ControlSize::Small => look::icon_button(name.clone(), ButtonKind::Outlined, button, IconName::Plus),
            ControlSize::Regular | ControlSize::Large => {
                look::button(name.clone(), ButtonKind::Outlined, button, "Add")
            }
        };
        let band = h_flex()
            .w(px(BAND_WIDTH))
            .h(height)
            .pl(Space::Medium)
            .pr(Space::XSmall)
            .bg(colors.raised)
            .when(edge, |band| band.line_under(cx))
            .typeset(kind)
            .child(div().flex_1().child("What it says"))
            .child(button)
            .id(place)
            .test_support();
        COLUMNS.row(name, div().w(px(COLUMNS.sample)).child(band), tall, cx)
    };
    samples().into_iter().map(row).collect()
}

/// At the app's scale, as the screens are, with where it draws each size's band.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let bars: Vec<String> = BarSize::ALL.iter().map(|bar| bar.written()[0].clone()).collect();
    let rows_named: Vec<String> = RowSize::ALL.iter().map(|row| row.written()[0].clone()).collect();
    COLUMNS.draw(text_system, &[(BARS, &bars), (ROWS, &rows_named)], rows)
}
