//! Clicking, the way a hand does, and any other event from the pointer.

#![cfg(test)]

use gpui_kit::base::test_support::snapshots;
use gpui_kit::test::{ElementSnapshot, TestEventExt as _, TestWindowExt as _};
use gpui_kit::{App, AppContext as _, ElementId, InputEvent, MouseButton, Pixels, Point, Window, point, px};

use super::Ui;

/// Clicks it as a hand does: the pointer comes to its middle, then moves a pixel while the button is down, as it does
/// in a quick press, though not far enough to start a drag. A click that didn't move would miss what goes wrong only
/// when the pointer moves under a press, such as a button that goes when the row it's on stops being hovered. Like
/// the kit's click, it finds it in a fresh frame and only clicks it if it can be seen, so the press can't land on
/// whatever is over it or in its place. It refuses one the frame drew off, as [`pressable`] says.
pub(crate) fn click(window: &mut Window, id: impl Into<ElementId>, cx: &mut App) {
    let id = id.into();
    window.render_frame(cx);
    let target = window.find(id.clone());
    pressable(&target, &format!("{id:?}"));
    click_at(window, target.bounds().center(), cx);
}

/// Fails unless a press on `target` would reach it: it can be seen, and the frame didn't draw it off. A press on a
/// button that's off does nothing, so a test that clicked one too soon would go on to wait for what the click never
/// started, and fail there, at its time limit, saying only what didn't come. Failing here says which button, and that
/// the test has to wait for it to come on first, with `Ui::on`. A test of what a press on one that's off does presses
/// it with [`click_at`], which presses wherever it's told.
pub(crate) fn pressable(target: &ElementSnapshot, named: &str) {
    assert!(target.visible(), "{named} can't be seen, so it can't be clicked");
    assert!(
        target.disabled() != Some(true),
        "{named} is off, so a click on it would do nothing: wait for it to come on before clicking it"
    );
}

/// Clicks at `at` as a hand does, as [`click`] says, for a place with nothing of its own to find it by, such as the
/// backdrop around a dialog.
pub(crate) fn click_at(window: &mut Window, at: Point<Pixels>, cx: &mut App) {
    window.pointer_move(at, None, cx);
    window.pointer_down(at, MouseButton::Left, cx);
    let at = at + point(px(1.), px(0.));
    window.pointer_move(at, Some(MouseButton::Left), cx);
    window.pointer_up(at, MouseButton::Left, cx);
}

/// Every element that shows under the middle of `id` in the frame drawn last, which is where [`click`] presses, by its
/// path and label, for a test whose click didn't do what it should to say what took it instead.
pub(crate) fn under_the_middle_of(window: &Window, id: impl Into<ElementId>) -> String {
    let at = window.find(id).bounds().center();
    let under = snapshots(window).into_iter().filter(|element| element.visible() && element.bounds().contains(&at));
    let named = under.map(|element| format!("{:?} {:?}", element.path().last(), element.label()));
    named.collect::<Vec<_>>().join(", ")
}

impl Ui<'_> {
    /// Clicks it as a hand does, with [`click`].
    pub(crate) fn click(&mut self, id: impl Into<ElementId>) {
        let id = id.into();
        self.frame();
        self.cx.update_window(self.handle, |_, window, cx| click(window, id, cx)).unwrap();
        self.frame();
    }

    /// Clicks at `at` as a hand does, with [`click_at`].
    pub(crate) fn click_at(&mut self, at: Point<Pixels>) {
        self.frame();
        self.cx.update_window(self.handle, |_, window, cx| click_at(window, at, cx)).unwrap();
        self.frame();
    }

    /// Sends the window an event from the pointer, and lets the app draw what it changed.
    pub(crate) fn send(&mut self, event: impl InputEvent) {
        let event = event.to_platform_input();
        self.cx.update_window(self.handle, |_, window, cx| window.dispatch_event(event, cx)).unwrap();
        self.frame();
    }
}

mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui_kit::{
        AnyWindowHandle, AppContext as _, Context, IntoElement, ParentElement as _, Render, TestAppContext, Window, div,
    };
    use look::{ButtonKind, ControlSize};

    use super::click;

    /// A button that's on and one that's off, each counting its presses.
    struct Two(Rc<Cell<usize>>);

    impl Render for Two {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let button = |id: &'static str| {
                let presses = self.0.clone();
                look::button(id, ButtonKind::Outlined, ControlSize::Regular, id)
                    .on_click(move |_, _, _| presses.set(presses.get() + 1))
            };
            div().child(button("on")).child(button("off").off(true))
        }
    }

    /// The window with both, and how often either has been pressed.
    fn two(cx: &mut TestAppContext) -> (AnyWindowHandle, Rc<Cell<usize>>) {
        cx.update(gpui_kit::init);
        cx.update(look::apply);
        let presses = Rc::new(Cell::new(0));
        let window = cx.add_window({
            let presses = presses.clone();
            move |_, _| Two(presses)
        });
        (window.into(), presses)
    }

    #[gpui_kit::test]
    fn a_click_presses_a_button_thats_on(cx: &mut TestAppContext) {
        let (window, presses) = two(cx);

        cx.update_window(window, |_, window, cx| click(window, "on", cx)).unwrap();

        assert_eq!(presses.get(), 1);
    }

    #[gpui_kit::test]
    #[should_panic(expected = "Name(\"off\") is off, so a click on it would do nothing")]
    fn a_click_on_a_button_thats_off_fails_and_names_it(cx: &mut TestAppContext) {
        let (window, _) = two(cx);

        cx.update_window(window, |_, window, cx| click(window, "off", cx)).unwrap();
    }
}
