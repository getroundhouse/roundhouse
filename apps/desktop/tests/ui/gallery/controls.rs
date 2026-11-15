//! Every size of a button, in one picture, as `look::ControlSize` has them: each by its name, as an outlined button
//! with words and an icon, the main button and an outlined button that's only an icon, so each one's edges show, with
//! how tall it is, the room beside its words and their kind of text, as the spec's table writes them.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{IconName, h_flex, v_flex};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem, Render, Styled as _,
    TestSupportExt as _, Window, div, px, size,
};
use look::{ControlSize, Sizing as _, Type, Typeset as _};

use super::capture::Picture;
use super::headless;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "controls";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "components.md";
/// What the spec's table calls the type that names each.
const KIND: &str = "look::ControlSize";
const NAME_WIDTH: f32 = 90.;
/// Wide enough for the three buttons of the largest size, apart.
const SAMPLE_WIDTH: f32 = 260.;
const WRITTEN_WIDTH: f32 = 150.;
const ROW: f32 = 56.;
const MARGIN: f32 = 16.;

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let row = |&control: &ControlSize| {
            let [name, height, padding, kind] = control.written();
            let place = places::id(KIND, &name);
            let id = |what: &str| format!("{control:?}-{what}");
            let name = div().w(px(NAME_WIDTH)).typeset(Type::Detail).text_color(colors.text_muted).child(name);
            let words = Button::new(id("words")).outline().sized(control).icon(IconName::Plus).label("Add");
            let main = Button::new(id("main")).primary().sized(control).label("Create");
            let square = Button::new(id("square")).outline().sized_square(control).icon(IconName::Close);
            let sample =
                h_flex().w(px(SAMPLE_WIDTH)).gap_3().child(words).child(main).child(square).id(place).test_support();
            let written = div()
                .w(px(WRITTEN_WIDTH))
                .typeset(Type::Detail)
                .text_color(colors.text_faint)
                .child(format!("{height} · {padding} beside · {kind}"));
            h_flex().h(px(ROW)).border_b_1().border_color(colors.edge).child(name).child(sample).child(written)
        };
        v_flex()
            .size_full()
            .bg(colors.pane)
            .text_color(colors.text)
            .p(px(MARGIN))
            .children(ControlSize::ALL.iter().map(row))
    }
}

/// At the app's scale, as the screens are, with where it draws each size's buttons.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    #[expect(clippy::cast_precision_loss, reason = "a handful of rows")]
    let rows = ControlSize::ALL.len() as f32;
    let width = px(MARGIN * 2. + NAME_WIDTH + SAMPLE_WIDTH + WRITTEN_WIDTH);
    let names: Vec<String> = ControlSize::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let size = size(width, px(MARGIN * 2. + rows * ROW));
    headless::draw_and_measure(&mut cx, size, None, |_| Sheet, |window| Places::of(window, &[(KIND, &names)]))
}
