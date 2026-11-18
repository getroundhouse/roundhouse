//! Every kind of bar, in one picture, as `look::BarKind` has them, stacked as the sidebar of changes stacks them: the
//! pull request's bar, a row of tabs, and the bar over Changes' comments, so what's in each starts where the tabs'
//! names do.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem, Render,
    Styled as _, TestSupportExt as _, Window, div, px, size,
};
use look::{BarKind, BarSize, ButtonKind, ControlSize, TabWidth, Type, Typeset as _};

use super::capture::Picture;
use super::headless;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "bar-kinds";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "components.md";
const KIND: &str = "look::BarKind";
const MARGIN: f32 = 32.;
/// The sidebar's width, about.
const WIDTH: f32 = 400.;

/// `bar`, found by its kind's name.
fn named(kind: BarKind, bar: look::Bar) -> AnyElement {
    let [name, ..] = kind.written();
    div().id(places::id(KIND, &name)).test_support().child(bar).into_any_element()
}

fn bars(cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let words = look::bar(BarKind::Words, BarSize::Regular)
        .of_work(look::Work::Open)
        .child(div().emphasis().text_color(look::Work::Open.text(colors)).child("#1879 Draw every tab"))
        .end(look::button("merge", ButtonKind::Edged, ControlSize::Regular, "Merge"));
    let tabs = look::bar(BarKind::Tabs, BarSize::Regular)
        .child(look::tab("all-files", "All files", TabWidth::Fits))
        .child(look::tab("changes", "Changes", TabWidth::Fits).count(8).showing(true));
    let comments = look::bar(BarKind::Words, BarSize::Regular)
        .child(div().emphasis().child("1 comment"))
        .end(look::button("send", ButtonKind::Main, ControlSize::Regular, "Send to agent"));
    v_flex()
        .w(px(WIDTH))
        .bg(colors.sidebar)
        .typeset(Type::Body)
        .text_color(colors.text)
        .child(named(BarKind::Words, words))
        .child(named(BarKind::Tabs, tabs))
        .child(comments)
        .into_any_element()
}

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p(px(MARGIN)).bg(look::colors(cx).pane).child(bars(cx))
    }
}

/// At the app's scale, as the screens are, with where it draws each kind.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    let kinds: Vec<String> = BarKind::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let size = size(px(MARGIN * 2. + WIDTH), px(MARGIN * 2. + 3. * 40.));
    headless::draw_and_measure(&mut cx, size, None, |_| Sheet, |window| Places::of(window, &[(KIND, &kinds)]))
}
