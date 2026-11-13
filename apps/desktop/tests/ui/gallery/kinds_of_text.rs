//! Every kind of text the app sets, in one picture, as `look::Type` has them: each by its name, set as itself, with
//! its font, its size and its weight as the spec's table writes them.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Context, IntoElement, ParentElement as _, PlatformTextSystem, Render, Styled as _, Window, div, px, size,
};
use look::{Type, Typeset as _};

use super::capture::Picture;
use super::headless;

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "type";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "look.md";
const NAME_WIDTH: f32 = 110.;
const SAMPLE_WIDTH: f32 = 320.;
const SET_WIDTH: f32 = 220.;
const ROW: f32 = 44.;
const MARGIN: f32 = 16.;

/// Something the app might say in each kind.
fn sample(kind: Type) -> &'static str {
    match kind {
        Type::PageTitle => "Notifications",
        Type::Heading => "Start a workspace",
        Type::Body => "Run the tests and fix what fails",
        Type::Detail => "2 minutes ago · 3 files",
        Type::Code => "cargo test --workspace",
        Type::Key => "F",
        Type::KeyHeld => "⌘",
    }
}

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let row = |&kind: &Type| {
            let [name, font, size, weight] = kind.written();
            let name = div().w(px(NAME_WIDTH)).typeset(Type::Detail).text_color(colors.text_muted).child(name);
            let sample = div().w(px(SAMPLE_WIDTH)).typeset(kind).child(sample(kind));
            let set = div()
                .w(px(SET_WIDTH))
                .typeset(Type::Detail)
                .text_color(colors.text_faint)
                .child(format!("{font} · {size} · {weight}"));
            h_flex().h(px(ROW)).border_b_1().border_color(colors.edge).child(name).child(sample).child(set)
        };
        v_flex().size_full().bg(colors.pane).text_color(colors.text).p(px(MARGIN)).children(Type::ALL.iter().map(row))
    }
}

/// At the app's scale, as the screens are.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> Picture {
    let mut cx = headless::app(text_system);
    #[expect(clippy::cast_precision_loss, reason = "a handful of rows")]
    let rows = Type::ALL.len() as f32;
    let width = px(MARGIN * 2. + NAME_WIDTH + SAMPLE_WIDTH + SET_WIDTH);
    headless::draw(&mut cx, size(width, px(MARGIN * 2. + rows * ROW)), |_| Sheet)
}
