//! Every kind of banner, in one picture, as `look::BannerKind` has them, each with what the app says in it: the hub's
//! error, setup that failed, and All files listing only some files.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem, Render,
    Styled as _, TestSupportExt as _, Window, div, px, size,
};
use look::{BannerKind, ButtonKind, ControlSize, Type, Typeset as _};

use super::capture::Picture;
use super::headless;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "banner-kinds";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "components.md";
const KIND: &str = "look::BannerKind";
const MARGIN: f32 = 32.;
const BETWEEN: f32 = 24.;
/// About a pane's width.
const WIDTH: f32 = 640.;

/// A banner of `kind`, with what the app says in one, found by its kind's name.
fn banner(kind: BannerKind) -> AnyElement {
    let [name, ..] = kind.written();
    let small =
        |id: &'static str, words: &'static str| look::button(id, ButtonKind::Borderless, ControlSize::Small, words);
    let drawn = match kind {
        BannerKind::Error => look::banner(kind)
            .child("Couldn't move the chat: the hub is older than this app")
            .answer(small("dismiss", "Dismiss")),
        BannerKind::Warning => look::banner(kind)
            .child("Setup failed, so the agent may find things missing.")
            .answer(small("log", "View log"))
            .answer(small("again", "Run again")),
        BannerKind::Note => look::banner(kind).child("There are so many files that some aren't shown"),
    };
    div().id(places::id(KIND, &name)).test_support().child(drawn).into_any_element()
}

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        div().size_full().p(px(MARGIN)).bg(colors.inset).child(
            v_flex()
                .w(px(WIDTH))
                .gap(px(BETWEEN))
                .py(px(BETWEEN))
                .bg(colors.pane)
                .typeset(Type::Body)
                .text_color(colors.text)
                .children(BannerKind::ALL.iter().map(|kind| banner(*kind))),
        )
    }
}

/// At the app's scale, as the screens are, with where it draws each kind.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    let kinds: Vec<String> = BannerKind::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let size = size(px(MARGIN * 2. + WIDTH), px(MARGIN * 2. + BETWEEN * 4. + 32. * 3.));
    headless::draw_and_measure(&mut cx, size, None, |_| Sheet, |window| Places::of(window, &[(KIND, &kinds)]))
}
