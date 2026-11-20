use gpui_kit::component::IconName;
use gpui_kit::component::button::Button as KitButton;
use gpui_kit::{CursorStyle, DefiniteLength, Length, Styled as _, TestAppContext, px};

use super::{Button, ButtonKind, Sizing as _, button, icon_button};
use crate::ControlSize;
use crate::spec_table;

#[test]
fn a_button_with_words_is_as_tall_as_its_size_with_its_room_at_each_end() {
    for &size in ControlSize::ALL {
        let mut sized = KitButton::new("words").sized(size);
        let style = sized.style();

        assert_eq!(style.size.height, Some(Length::from(size.height())), "{size:?}");
        assert_eq!(style.size.width, None, "{size:?}: as wide as its words");
        let room = Some(DefiniteLength::from(size.padding()));
        assert_eq!([style.padding.left, style.padding.right], [room; 2], "{size:?}");
    }
}

#[test]
fn a_button_thats_only_an_icon_is_a_square_of_its_size_with_no_room_taken() {
    for &size in ControlSize::ALL {
        let mut sized = KitButton::new("icon").sized_square(size);
        let style = sized.style();

        let side = Some(Length::from(size.height()));
        assert_eq!([style.size.width, style.size.height], [side; 2], "{size:?}");
        let none = Some(DefiniteLength::from(px(0.)));
        let padding = &style.padding;
        assert_eq!([padding.top, padding.right, padding.bottom, padding.left], [none; 4], "{size:?}");
    }
}

#[test]
fn the_specs_table_has_every_kind_of_button_in_order() {
    let table: Vec<[String; 1]> = spec_table::rows("**Every kind of button, by its name.**");

    let kinds: Vec<[String; 1]> = ButtonKind::ALL.iter().map(|kind| kind.written()).collect();
    assert_eq!(
        table, kinds,
        "docs/spec/components.md's table of kinds of button, against crates/look/src/controls/kinds.rs"
    );
}

/// The hand is the button's own, so none is left with the arrow where a press does something, or with the hand where it
/// does nothing.
#[gpui_kit::test]
fn a_button_has_the_hand_only_while_a_press_does_something(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(crate::theme::apply);

    cx.update(|cx| {
        let cursor = |button: Button| button.drawn(cx).style().mouse_cursor;
        let made = || button("words", ButtonKind::Borderless, ControlSize::Small, "Add");

        assert_eq!(cursor(made()), Some(CursorStyle::PointingHand));
        assert_eq!(
            cursor(icon_button("icon", ButtonKind::Main, ControlSize::Small, IconName::Plus)),
            Some(CursorStyle::PointingHand)
        );
        assert_eq!(cursor(made().off(true)), None, "off");
        assert_eq!(cursor(made().waiting(true)), None, "waiting");
    });
}

/// A quiet or faint button's words are its kind's colour, but while it's off, when they're the kit's greyed-out
/// colour, which a colour of the button's own would cover.
#[gpui_kit::test]
fn a_quiet_buttons_words_are_muted_but_while_its_off(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(crate::theme::apply);

    cx.update(|cx| {
        let colors = crate::colors::colors(cx).clone();
        let words = |kind, off| button("words", kind, ControlSize::Small, "Add").off(off).drawn(cx).style().text.color;

        assert_eq!(words(ButtonKind::Quiet, false), Some(colors.text_muted));
        assert_eq!(words(ButtonKind::Faint, false), Some(colors.text_faint));
        assert_eq!(words(ButtonKind::Borderless, false), None, "the kit's own");
        assert_eq!(words(ButtonKind::Quiet, true), None, "off");
    });
}

/// A button the pointer can rest on, alone in a window, so the only raised fill painted is its tooltip's.
struct Resting;

impl gpui_kit::Render for Resting {
    fn render(&mut self, _: &mut gpui_kit::Window, _: &mut gpui_kit::Context<Self>) -> impl gpui_kit::IntoElement {
        use gpui_kit::ParentElement as _;
        gpui_kit::div()
            .p(px(40.))
            .child(button("says", ButtonKind::Borderless, ControlSize::Regular, "Add").tooltip("Adds one"))
    }
}

/// Nothing rests on what's touched, so a button under touch has no tooltip, and one with a pointer says what it does.
#[gpui_kit::test]
fn a_button_says_what_it_does_while_the_pointer_rests_on_it_and_not_under_touch(cx: &mut TestAppContext) {
    use gpui_kit::{AppContext as _, VisualTestContext};

    cx.update(gpui_kit::init);
    cx.update(crate::theme::apply);
    for (input, shown) in [(crate::Input::Pointer, true), (crate::Input::Touch, false)] {
        cx.update(|cx| input.choose(cx));
        let window = cx.update(|cx| {
            cx.open_window(gpui_kit::WindowOptions::default(), |window, cx| {
                let view = cx.new(|_| Resting);
                cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
            })
            .unwrap()
        });
        let cx = &mut VisualTestContext::from_window(window.into(), cx);
        cx.update(|window, cx| window.draw(cx).clear(cx));

        cx.simulate_mouse_move(gpui_kit::point(px(50.), px(50.)), None, gpui_kit::Modifiers::default());
        cx.executor().advance_clock(std::time::Duration::from_secs(1));
        cx.run_until_parked();
        let raised = cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            let raised: gpui_kit::Background = crate::colors::colors(cx).raised.into();
            window.painted_quads().iter().any(|quad| quad.background == raised)
        });

        assert_eq!(raised, shown, "{input:?}");
    }
}
