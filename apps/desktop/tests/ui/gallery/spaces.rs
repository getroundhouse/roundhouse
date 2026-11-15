//! Every step of space, in one picture, as `look::Space` has it: each by its name, as two raised blocks with the step
//! between them, so the room shows, and how much room it is, as the spec's table writes it.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem, Render,
    Styled as _, TestSupportExt as _, Window, div, px, size,
};
use look::{Corner, Shaped as _, Space, Type, Typeset as _};

use super::capture::Picture;
use super::headless;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "spaces";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "look.md";
/// What the spec's table calls the type that names each step.
const KIND: &str = "look::Space";
const NAME_WIDTH: f32 = 90.;
/// Wide enough for two blocks and the largest step between them.
const SAMPLE_WIDTH: f32 = 140.;
const WRITTEN_WIDTH: f32 = 50.;
const BLOCK: f32 = 32.;
const ROW: f32 = 48.;
const MARGIN: f32 = 16.;

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let block = || div().size(px(BLOCK)).corners(Corner::Glyph).bg(colors.raised);
        let row = |space: Space| -> AnyElement {
            let [name, room] = space.written();
            let place = places::id(KIND, &name);
            let label = div().w(px(NAME_WIDTH)).typeset(Type::Detail).text_color(colors.text_muted).child(name);
            let sample = h_flex().w(px(SAMPLE_WIDTH)).gap(space).child(block()).child(block()).id(place).test_support();
            let written = div().w(px(WRITTEN_WIDTH)).typeset(Type::Detail).text_color(colors.text_faint).child(room);
            h_flex()
                .h(px(ROW))
                .border_b_1()
                .border_color(colors.edge)
                .child(label)
                .child(sample)
                .child(written)
                .into_any_element()
        };
        v_flex()
            .size_full()
            .bg(colors.pane)
            .text_color(colors.text)
            .p(px(MARGIN))
            .children(Space::ALL.iter().copied().map(row))
    }
}

/// At the app's scale, as the screens are, with where it draws each step's blocks.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    #[expect(clippy::cast_precision_loss, reason = "a handful of rows")]
    let rows = Space::ALL.len() as f32;
    let width = px(MARGIN * 2. + NAME_WIDTH + SAMPLE_WIDTH + WRITTEN_WIDTH);
    let names: Vec<String> = Space::ALL.iter().map(|space| space.written()[0].clone()).collect();
    let size = size(width, px(MARGIN * 2. + rows * ROW));
    headless::draw_and_measure(&mut cx, size, None, |_| Sheet, |window| Places::of(window, &[(KIND, &names)]))
}
