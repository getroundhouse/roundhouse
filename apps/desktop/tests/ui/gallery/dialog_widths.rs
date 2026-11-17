//! Every width of dialog, in one picture, as `look::DialogWidth` has them: each by its name, with a dialog of that width
//! as the app draws it, laid out where it's put rather than over a window.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, PlatformTextSystem, Render, Styled as _,
    TestSupportExt as _, Window, div, px, size,
};
use look::{ButtonKind, ControlSize, DialogWidth, Type, Typeset as _};

use super::capture::Picture;
use super::headless;
use super::places::{self, Places};

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "dialog-widths";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "components.md";
/// What the spec's table calls the type that names each.
const KIND: &str = "look::DialogWidth";
const NAME_WIDTH: f32 = 90.;
const ROW: f32 = 200.;
const MARGIN: f32 = 24.;

/// What a dialog of `width` is for, as the spec's table has it, with its answers.
fn asked(width: DialogWidth) -> look::Dialog {
    let answer = |id: &str, kind, words| look::button(format!("{width:?}-{id}"), kind, ControlSize::Large, words);
    match width {
        DialogWidth::Small => look::dialog("Close this chat?")
            .lead("Its agent is still working, and closing the chat stops it. The history stays.")
            .closes(answer("keep", ButtonKind::Edged, "Cancel"))
            .confirms(answer("act", ButtonKind::Main, "Close chat")),
        DialogWidth::Regular => look::dialog("Disconnect Personal?")
            .lead("The hub forgets its token, so connecting it again takes pasting one again.")
            .closes(answer("keep", ButtonKind::Edged, "Cancel"))
            .confirms(answer("act", ButtonKind::Removes, "Disconnect")),
        DialogWidth::Wide => look::dialog("Pair a device with this Mac")
            .lead("Scan on a phone, or choose Connect another hub… on another computer and paste this link.")
            .closes(answer("keep", ButtonKind::Edged, "Cancel"))
            .answer(answer("copy", ButtonKind::Main, "Copy link")),
    }
    .width(width)
}

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = look::colors(cx);
        let row = |&width: &DialogWidth| {
            let [name, _] = width.written();
            let place = places::id(KIND, &name);
            h_flex()
                .h(px(ROW))
                .items_center()
                .child(div().w(px(NAME_WIDTH)).typeset(Type::Detail).text_color(colors.text_muted).child(name))
                .child(div().id(place).test_support().child(look::shown_dialog(asked(width), cx)))
        };
        v_flex()
            .size_full()
            .bg(colors.pane)
            .text_color(colors.text)
            .p(px(MARGIN))
            .children(DialogWidth::ALL.iter().map(row))
    }
}

/// At the app's scale, as the screens are, with where it draws each width's dialog.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> (Picture, Places) {
    let mut cx = headless::app(text_system);
    #[expect(clippy::cast_precision_loss, reason = "a handful of rows")]
    let rows = DialogWidth::ALL.len() as f32;
    let widest = DialogWidth::ALL.iter().map(|width| f32::from(width.width())).fold(0., f32::max);
    let width = px(MARGIN * 2. + NAME_WIDTH + widest);
    let names: Vec<String> = DialogWidth::ALL.iter().map(|each| each.written()[0].clone()).collect();
    let size = size(width, px(MARGIN * 2. + rows * ROW));
    headless::draw_and_measure(&mut cx, size, None, |_| Sheet, |window| Places::of(window, &[(KIND, &names)]))
}
