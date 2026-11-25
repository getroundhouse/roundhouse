//! The keyboard's focus in a card that shows on it, where the card has buttons.

use gpui_kit::{
    Context, Entity, FocusHandle, InteractiveElement as _, IntoElement, Modifiers, ParentElement as _, Render, Role,
    Styled as _, TestAppContext, VisualTestContext, Window, div, point, px,
};

use super::HoverCard;
use super::tests::{CLOSES, OPENS, move_to, settle, showing, tab};

/// A square whose card has two buttons, the first until it goes, with a button before it and one after it.
struct WithButtons {
    buttons: [FocusHandle; 4],
    gone: bool,
}

impl Render for WithButtons {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let [before, first, second, after] =
            self.buttons.clone().map(|focus| div().size(px(10.)).track_focus(&focus.tab_stop(true)));
        let first = (!self.gone).then_some(first);
        let card = HoverCard::new("card", OPENS, CLOSES)
            .trigger(div().debug_selector(|| "trigger".into()).size(px(20.)))
            .on_focus(Role::Group, "Square", "A card")
            .content(move |_, _, _| div().debug_selector(|| "card".into()).p(px(10.)).children(first).child(second));
        div().p(px(200.)).child(before).child(card).child(after)
    }
}

fn with_buttons(cx: &mut TestAppContext) -> (&mut VisualTestContext, Entity<WithButtons>, [FocusHandle; 4]) {
    cx.update(crate::bind_hover_card_keys);
    let buttons = cx.update(|cx| std::array::from_fn(|_| cx.focus_handle()));
    let made = WithButtons { buttons: buttons.clone(), gone: false };
    let (made, cx) = cx.add_window_view(move |_, _| made);
    cx.update(|window, cx| {
        window.activate_window();
        window.focus(&buttons[0], cx);
        window.draw(cx).clear(cx);
    });
    // The window says where the pointer is as it comes to the front, so before a test moves it.
    settle(cx, CLOSES);
    (cx, made, buttons)
}

/// Presses `key`, and says which of `buttons` has the focus then.
fn focused_by(cx: &mut VisualTestContext, key: &str, buttons: &[FocusHandle; 4]) -> Option<usize> {
    cx.simulate_keystrokes(key);
    settle(cx, CLOSES * 2);
    cx.update(|window, _| buttons.iter().position(|button| button.is_focused(window)))
}

#[gpui_kit::test]
fn tab_goes_from_what_a_card_is_for_through_its_buttons_and_on_and_shift_tab_comes_back(cx: &mut TestAppContext) {
    let (cx, _, buttons) = with_buttons(cx);

    tab(cx);
    assert!(showing(cx));
    assert_eq!(["tab", "tab", "shift-tab"].map(|key| focused_by(cx, key, &buttons)), [Some(1), Some(2), Some(1)]);
    assert!(showing(cx), "for as long as the focus is in it");
    assert_eq!(focused_by(cx, "shift-tab", &buttons), None, "what it's for");
    assert!(showing(cx));
    assert_eq!(["tab", "tab", "tab"].map(|key| focused_by(cx, key, &buttons)), [Some(1), Some(2), Some(3)]);

    assert!(!showing(cx), "past the last, to what's after what it's for");
}

#[gpui_kit::test]
fn a_button_that_goes_with_the_focus_leaves_the_focus_on_what_its_card_is_for(cx: &mut TestAppContext) {
    let (cx, made, buttons) = with_buttons(cx);
    tab(cx);
    assert_eq!(focused_by(cx, "tab", &buttons), Some(1));

    cx.update(|_, cx| {
        made.update(cx, |made, cx| {
            made.gone = true;
            cx.notify();
        });
    });
    settle(cx, CLOSES * 2);

    assert!(showing(cx));
    assert_eq!(focused_by(cx, "tab", &buttons), Some(2));
}

#[gpui_kit::test]
fn a_press_on_a_card_leaves_the_focus_where_it_is(cx: &mut TestAppContext) {
    let (cx, _, buttons) = with_buttons(cx);
    let square = cx.debug_bounds("trigger").unwrap().center();
    move_to(cx, square.x.as_f32(), square.y.as_f32(), OPENS);
    let card = cx.debug_bounds("card").expect("the card shows");

    cx.simulate_click(card.origin + point(px(2.), px(2.)), Modifiers::default());
    settle(cx, CLOSES);

    assert!(cx.update(|window, _| buttons[0].is_focused(window)));
}
