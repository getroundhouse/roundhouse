//! A field of a panel on a page of its own, such as the one a token is pasted into, which does what the panel does as
//! Enter is pressed in it, and how it's drawn: what it's called above it, and what goes in it under it.

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::v_flex;
use gpui_kit::{
    App, AppContext as _, Context, Div, Entity, InteractiveElement as _, ParentElement as _, Pixels, Styled as _,
    Subscription, TestSupportExt as _, Window, div, px,
};
use look::Typeset as _;

/// How wide a field is at most, so a token's isn't as wide as a wide window.
pub(super) const FIELDS_WIDTH: Pixels = px(560.);

/// A field, kept by the page's state `T`, which has `entered` done to it as Enter is pressed in the field.
pub(super) struct Field {
    pub(super) input: Entity<InputState>,
    _entered: Subscription,
}

impl Field {
    /// A field made with `made`, with the cursor in it.
    pub(super) fn new<T: 'static>(
        window: &mut Window,
        cx: &mut Context<T>,
        made: impl FnOnce(InputState) -> InputState,
        entered: fn(&mut T, &mut Window, &mut Context<T>),
    ) -> Field {
        let input = cx.new(|cx| made(InputState::new(window, cx)));
        let entered = cx.subscribe_in(&input, window, move |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                entered(this, window, cx);
            }
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        Field { input, _entered: entered }
    }

    /// A field for a token, whose text shows only as dots, and can't be copied from.
    pub(super) fn token<T: 'static>(
        window: &mut Window,
        cx: &mut Context<T>,
        entered: fn(&mut T, &mut Window, &mut Context<T>),
    ) -> Field {
        Field::new(window, cx, |input| input.masked(true), entered)
    }

    /// Whether anything but spaces is in it.
    pub(super) fn filled(&self, cx: &App) -> bool {
        !self.input.read(cx).value().trim().is_empty()
    }

    pub(super) fn value(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }
}

/// A field `called` something, with what goes in it said under it. `id` is its own, for tests to find it by.
pub(super) fn labelled(called: &'static str, id: &'static str, input: Input, says: &'static str, cx: &App) -> Div {
    let colors = look::colors(cx);
    let input = div().id(id).child(input.aria_label(called)).test_support();
    v_flex()
        .w_full()
        .max_w(FIELDS_WIDTH)
        .gap(look::Space::Small)
        .child(div().emphasis().child(called))
        .child(input)
        .child(div().typeset(look::Type::Detail).text_color(colors.text_faint).child(says))
}
