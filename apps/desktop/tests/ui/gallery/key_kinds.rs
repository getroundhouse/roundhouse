//! Every kind of key, in one picture, as `look::KeyKind` has them: each by its name, with the key that finds a
//! setting, Return and Shift-Tab, as this system writes them.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem, Render, Styled as _,
    TestSupportExt as _, Window, div, px, size,
};
use look::{KeyKind, Type, Typeset as _};

use super::capture::Picture;
use super::headless;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "key-kinds";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "components.md";
/// What the spec's table calls the type that names each.
const KIND: &str = "look::KeyKind";
/// The keys each kind is drawn with: one with something held, one on its own, and one with Shift held.
const KEYS: [&str; 3] = ["secondary-f", "enter", "shift-tab"];
const NAME_WIDTH: f32 = 90.;
const SAMPLE_WIDTH: f32 = 180.;
const ROW: f32 = 48.;
const MARGIN: f32 = 16.;

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let row = |&kind: &KeyKind| {
            let [name] = kind.written();
            let place = places::id(KIND, &name);
            let name = div().w(px(NAME_WIDTH)).typeset(Type::Detail).text_color(colors.text_muted).child(name);
            let keys = h_flex()
                .gap_4()
                .typeset(Type::Body)
                .children(KEYS.map(|key| look::key(kind, key)))
                .id(place)
                .test_support();
            h_flex()
                .h(px(ROW))
                .border_b_1()
                .border_color(colors.edge)
                .child(name)
                .child(div().w(px(SAMPLE_WIDTH)).child(keys))
        };
        v_flex()
            .size_full()
            .bg(colors.pane)
            .text_color(colors.text)
            .p(px(MARGIN))
            .children(KeyKind::ALL.iter().map(row))
    }
}

/// At the app's scale, as the screens are, with where it draws each kind's keys.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    #[expect(clippy::cast_precision_loss, reason = "a handful of rows")]
    let rows = KeyKind::ALL.len() as f32;
    let width = px(MARGIN * 2. + NAME_WIDTH + SAMPLE_WIDTH);
    let names: Vec<String> = KeyKind::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let size = size(width, px(MARGIN * 2. + rows * ROW));
    headless::draw_and_measure(&mut cx, size, None, |_| Sheet, |window| Places::of(window, &[(KIND, &names)]))
}
