//! Every width of tab, in one picture, as `look::TabWidth` has them, each in a bar of its own as the app draws one: the
//! even tabs of a workspace, one showing, one under the pointer with its close button and one that needs a look, then
//! the sidebar's tabs that fit their names, one showing with its count and one under the pointer. The pointer really is
//! over each one under the pointer, one at a time, so each is drawn as the app draws it.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{Icon, IconName, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem,
    Render, Styled as _, TestSupportExt as _, Window, div, px, size,
};
use look::{ButtonKind, ControlSize, IconSizing as _, TabWidth, Type, Typeset as _};

use super::capture::Picture;
use super::headless::{self, Pointer};
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "tab-widths";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "components.md";
const WIDTH: &str = "look::TabWidth";
const MARGIN: f32 = 32.;
const BETWEEN: f32 = 28.;
/// Wide enough for four even tabs and a little room after them.
const EVEN_BAR: f32 = 840.;
/// The sidebar's width, about.
const FITS_BAR: f32 = 400.;
/// The tabs under the pointer, by the ID of the box the pointer goes to the middle of.
const POINTED: [&str; 2] = ["even-pointed-tab", "fits-pointed-tab"];

/// A tab's close button, as a workspace's tabs have.
fn close(id: &'static str) -> look::Button {
    look::icon_button(id, ButtonKind::Borderless, ControlSize::Small, IconName::Close).tooltip("Close chat")
}

/// A bar of tabs, as tall as a regular bar, with its bottom edge under them.
fn bar(width: f32, tabs: impl IntoIterator<Item = AnyElement>, cx: &App) -> AnyElement {
    h_flex()
        .relative()
        .w(px(width))
        .h(look::BarSize::Regular.height())
        .pl(look::Space::XSmall)
        .child(div().absolute().left_0().right_0().bottom_0().h(look::EDGE).bg(look::colors(cx).edge))
        .children(tabs)
        .into_any_element()
}

fn even(cx: &App) -> AnyElement {
    let mark = || look::chat_mark(look::Signal::Idle, look::Size::Inline, cx);
    let tabs = [
        look::tab("even-showing", "Which colours does the app use", TabWidth::Even)
            .mark(mark())
            .showing(true)
            .closes(close("close-showing")),
        look::tab("even-pointed", "Why does the hub restart", TabWidth::Even)
            .mark(mark())
            .closes(close("close-pointed")),
        look::tab("even-needs-a-look", "The sidebar's records", TabWidth::Even)
            .mark(mark())
            .needs_a_look(true)
            .closes(close("close-needs-a-look")),
        look::tab("even-terminal", "Terminal", TabWidth::Even)
            .mark(Icon::new(gpui_kit::assets::IconName::Terminal).sized(look::IconSize::Large))
            .closes(close("close-terminal")),
    ];
    bar(EVEN_BAR, tabs.map(IntoElement::into_any_element), cx)
}

fn fits(cx: &App) -> AnyElement {
    let tabs = [
        look::tab("fits-pointed", "All files", TabWidth::Fits),
        look::tab("fits-showing", "Changes", TabWidth::Fits).count(8).showing(true),
    ];
    bar(FITS_BAR, tabs.map(IntoElement::into_any_element), cx)
}

/// `sample`, under its name, faint, with its place found by its width's name.
fn named(width: TabWidth, sample: AnyElement, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let [name, _] = width.written();
    v_flex()
        .gap(look::Space::Small)
        .child(div().typeset(Type::Detail).text_color(colors.text_muted).child(name.clone()))
        .child(div().id(places::id(WIDTH, &name)).test_support().child(sample))
        .into_any_element()
}

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        v_flex()
            .size_full()
            .gap(px(BETWEEN))
            .p(px(MARGIN))
            .bg(colors.pane)
            .text_color(colors.text)
            .child(named(TabWidth::Even, even(cx), cx))
            .child(named(TabWidth::Fits, fits(cx), cx))
    }
}

/// At the app's scale, as the screens are, with where it draws each width.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    let widths: Vec<String> = TabWidth::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let size = size(px(MARGIN * 2. + EVEN_BAR), px(MARGIN * 2. + BETWEEN + 2. * (40. + 24.)));
    let pointed: Vec<(ElementId, Pointer)> =
        POINTED.iter().map(|id| (ElementId::Name((*id).into()), Pointer::Over)).collect();
    headless::draw_pointing(&mut cx, size, None, |_| Sheet, &pointed, |window| Places::of(window, &[(WIDTH, &widths)]))
}
