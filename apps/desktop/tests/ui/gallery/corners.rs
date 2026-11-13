//! Every corner the app rounds, in one picture, as `look::Corner` has them: each by its name, on a raised square with
//! the one thin edge, with how round it is as the spec's table writes it.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Context, IntoElement, ParentElement as _, PlatformTextSystem, Render, Styled as _, Window, div, px, size,
};
use look::{Corner, Shaped as _, Type, Typeset as _};

use super::capture::Picture;
use super::headless;

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "corners";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "look.md";
const NAME_WIDTH: f32 = 130.;
/// Big enough that the roundest corner is plain, and the least still shows.
const SQUARE: f32 = 40.;
const SAMPLE_WIDTH: f32 = 90.;
const RADIUS_WIDTH: f32 = 60.;
const ROW: f32 = 56.;
const MARGIN: f32 = 16.;

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let row = |&corner: &Corner| {
            let [name, radius] = corner.written();
            let name = div().w(px(NAME_WIDTH)).typeset(Type::Detail).text_color(colors.text_muted).child(name);
            let square = div()
                .size(px(SQUARE))
                .corners(corner)
                .border(look::EDGE)
                .border_color(colors.control_edge)
                .bg(colors.raised);
            let sample = div().w(px(SAMPLE_WIDTH)).child(square);
            let radius = div().w(px(RADIUS_WIDTH)).typeset(Type::Detail).text_color(colors.text_faint).child(radius);
            h_flex().h(px(ROW)).border_b_1().border_color(colors.edge).child(name).child(sample).child(radius)
        };
        v_flex().size_full().bg(colors.pane).text_color(colors.text).p(px(MARGIN)).children(Corner::ALL.iter().map(row))
    }
}

/// At the app's scale, as the screens are.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> Picture {
    let mut cx = headless::app(text_system);
    #[expect(clippy::cast_precision_loss, reason = "a handful of rows")]
    let rows = Corner::ALL.len() as f32;
    let width = px(MARGIN * 2. + NAME_WIDTH + SAMPLE_WIDTH + RADIUS_WIDTH);
    headless::draw(&mut cx, size(width, px(MARGIN * 2. + rows * ROW)), |_| Sheet)
}
