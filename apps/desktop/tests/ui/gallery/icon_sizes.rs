//! Every size of an icon, in one picture, as `look::IconSize` has them: each by its name, as a few icons on raised
//! squares as big as they are, so each one's box shows, then one beside the kind of text it goes with, and how big it
//! is, as the spec's table writes it.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{Icon, IconName, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem, Render,
    Styled as _, TestSupportExt as _, Window, div, px, size,
};
use look::{IconSize, IconSizing as _, Space, Type, Typeset as _};

use super::capture::Picture;
use super::headless;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "icons";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "look.md";
/// What the spec's table calls the type that names each size.
const KIND: &str = "look::IconSize";
const NAME_WIDTH: f32 = 90.;
/// Wide enough for three of the largest, apart.
const SAMPLE_WIDTH: f32 = 90.;
/// Wide enough for the longest of the icons beside text.
const BESIDE_WIDTH: f32 = 150.;
const WRITTEN_WIDTH: f32 = 50.;
const ROW: f32 = 48.;
const MARGIN: f32 = 16.;

/// An icon beside the kind of text it goes with, at each size.
fn beside(size: IconSize) -> (IconName, &'static str, Type) {
    match size {
        IconSize::Small => (IconName::Check, "Up to date", Type::Detail),
        IconSize::Regular => (IconName::TriangleAlert, "Ends in 9 days", Type::Body),
        IconSize::Large => (IconName::Bot, "Explore", Type::Body),
    }
}

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let row = |icon: IconSize| -> AnyElement {
            let [name, pixels] = icon.written();
            let place = places::id(KIND, &name);
            let label = div().w(px(NAME_WIDTH)).typeset(Type::Detail).text_color(colors.text_muted).child(name);
            // Each in a place as wide as the largest, so the icons are in columns down the sheet.
            let boxed = |name: IconName| {
                let drawn = div().bg(colors.raised).child(Icon::new(name).sized(icon));
                div().w(IconSize::Large.size()).flex().justify_center().child(drawn)
            };
            let sample = h_flex()
                .w(px(SAMPLE_WIDTH))
                .gap(Space::Small)
                .children([IconName::Check, IconName::Folder, IconName::Close].map(boxed));
            let (name, words, kind) = beside(icon);
            let beside = h_flex()
                .w(px(BESIDE_WIDTH))
                .gap(Space::Small)
                .typeset(kind)
                .text_color(colors.text_muted)
                .child(Icon::new(name).sized(icon))
                .child(words);
            let written = div().w(px(WRITTEN_WIDTH)).typeset(Type::Detail).text_color(colors.text_faint).child(pixels);
            h_flex()
                .h(px(ROW))
                .border_b_1()
                .border_color(colors.edge)
                .child(label)
                .child(h_flex().child(sample).child(beside).id(place).test_support())
                .child(written)
                .into_any_element()
        };
        v_flex()
            .size_full()
            .bg(colors.pane)
            .text_color(colors.text)
            .p(px(MARGIN))
            .children(IconSize::ALL.iter().copied().map(row))
    }
}

/// At the app's scale, as the screens are, with where it draws each size's icons.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    #[expect(clippy::cast_precision_loss, reason = "a handful of rows")]
    let rows = IconSize::ALL.len() as f32;
    let width = px(MARGIN * 2. + NAME_WIDTH + SAMPLE_WIDTH + BESIDE_WIDTH + WRITTEN_WIDTH);
    let names: Vec<String> = IconSize::ALL.iter().map(|icon| icon.written()[0].clone()).collect();
    let size = size(width, px(MARGIN * 2. + rows * ROW));
    headless::draw_and_measure(&mut cx, size, None, |_| Sheet, |window| Places::of(window, &[(KIND, &names)]))
}
