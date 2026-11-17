//! Every kind of switch, in one picture, as `look::SwitchKind` has them: each by its name, as a switch of icons and of
//! words, with the first chosen, as the app draws it.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem, Render, Styled as _,
    TestSupportExt as _, Window, div, px, size,
};
use look::{SwitchKind, Type, Typeset as _};

use super::capture::Picture;
use super::headless;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "switch-kinds";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "components.md";
/// What the spec's table calls the type that names each.
const KIND: &str = "look::SwitchKind";
const NAME_WIDTH: f32 = 90.;
const ICONS_WIDTH: f32 = 160.;
const WORDS_WIDTH: f32 = 320.;
const ROW: f32 = 56.;
const MARGIN: f32 = 16.;

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let row = |&kind: &SwitchKind| {
            let [name] = kind.written();
            let place = places::id(KIND, &name);
            let id = |what: &str| format!("{kind:?}-{what}");
            let icons = look::switch(kind)
                .icon(id("one-column"), Lucide::TextAlignStart, "One column", true, |_, _| {})
                .icon(id("side-by-side"), Lucide::Columns2, "Side by side", false, |_, _| {})
                .icon(id("whole-file"), Lucide::FileText, "Whole file", false, |_, _| {});
            let words = look::switch(kind).words(id("account"), "Your account", true, |_, _| {}).words(
                id("organization"),
                "An organization",
                false,
                |_, _| {},
            );
            h_flex()
                .h(px(ROW))
                .border_b_1()
                .border_color(colors.edge)
                .child(div().w(px(NAME_WIDTH)).typeset(Type::Detail).text_color(colors.text_muted).child(name))
                // In rows, as a bar lays a switch, so a track is only as wide as its segments.
                .child(h_flex().w(px(ICONS_WIDTH)).child(h_flex().id(place).test_support().child(icons)))
                .child(h_flex().w(px(WORDS_WIDTH)).child(words))
        };
        v_flex()
            .size_full()
            .bg(colors.pane)
            .text_color(colors.text)
            .p(px(MARGIN))
            .children(SwitchKind::ALL.iter().map(row))
    }
}

/// At the app's scale, as the screens are, with where it draws each kind's switch of icons.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    #[expect(clippy::cast_precision_loss, reason = "a handful of rows")]
    let rows = SwitchKind::ALL.len() as f32;
    let width = px(MARGIN * 2. + NAME_WIDTH + ICONS_WIDTH + WORDS_WIDTH);
    let names: Vec<String> = SwitchKind::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let size = size(width, px(MARGIN * 2. + rows * ROW));
    headless::draw_and_measure(&mut cx, size, None, |_| Sheet, |window| Places::of(window, &[(KIND, &names)]))
}
