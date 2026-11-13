//! The strip along the window's top, which stands in for a title bar. Moving the window is the platform's, so it's
//! checked by hand. The test window can't move, and panics if anything asks it to, so these check what the strip
//! leaves alone, and on Windows, which presses go on to Windows, which moves the window from them.

#![cfg(test)]

use desktop::app::{TitleBar, the_title_bar_is};
use desktop::lost_release::{self, Button};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, DispatchEventResult, ElementId, InputEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, TestAppContext, point, px,
};

use crate::support::{Stack, Ui};
use crate::tools::in_a_new_workspace;

/// In the header, between the workspace's name and its buttons, where a title bar would be.
const ON_THE_STRIP: Point<Pixels> = point(px(700.), px(20.));

#[gpui_kit::test]
fn a_click_on_the_strip_leaves_the_composer_focused(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    ui.input("half");

    press(&mut ui, ON_THE_STRIP);
    send(&mut ui, up(ON_THE_STRIP));

    ui.input(" typed");
    assert_eq!(ui.composer_text(), "half typed");
}

#[gpui_kit::test]
fn a_drag_from_a_button_in_the_strip_is_the_buttons(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);

    // Let go off the button, so it isn't a click either.
    let from = center(&mut ui, "toggle-diff");
    let to = point(from.x, from.y + px(200.));
    ui.cx.update_window(ui.handle, |_, window, cx| window.drag(from, to, cx)).unwrap();
    ui.frame();
    assert!(ui.changes_open(), "the sidebar of changes is still open");

    ui.click("toggle-diff");
    assert!(!ui.changes_open());
}

/// GPUI's Windows backend hands Windows a press over the strip that the app lets propagate, and Windows takes it as
/// its title bar's, so one on a button has to stop.
#[gpui_kit::test]
fn on_windows_a_press_on_a_button_in_the_strip_stays_the_buttons(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = on_windows(cx, &stack);
    let on = center(&mut ui, "toggle-diff");

    let pressed = press(&mut ui, on);
    send(&mut ui, up(on));

    assert!(!pressed.propagate, "Windows doesn't hear the press");
    assert!(!ui.changes_open(), "the button is clicked");
}

/// The button that opens the menus opens them as it's pressed on Windows too, where the strip is the title bar, and
/// Windows doesn't hear the press, as it doesn't any button's. That the press goes no further, to what's under the
/// button, is `menus::drawn`'s to check.
#[cfg(not(target_os = "macos"))]
#[gpui_kit::test]
fn on_windows_a_press_on_the_button_that_opens_the_menus_opens_them_and_stays_its(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = on_windows(cx, &stack);
    let on = center(&mut ui, "menus");

    let pressed = press(&mut ui, on);
    send(&mut ui, up(on));

    assert!(!pressed.propagate, "Windows doesn't hear the press");
    assert!(ui.has(crate::support::MENU), "the menus are open");
}

/// A greyed-out button stops a press itself, so Windows doesn't move the window from it either.
#[gpui_kit::test]
fn on_windows_a_press_on_a_greyed_out_button_in_the_strip_stops_there(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = on_windows(cx, &stack);
    assert!(ui.disabled("regenerate-title"), "a new workspace has no title to make another from");
    let on = center(&mut ui, "regenerate-title");

    let pressed = press(&mut ui, on);

    assert!(!pressed.propagate, "Windows doesn't hear the press");
}

#[gpui_kit::test]
fn on_windows_a_press_on_the_empty_strip_goes_on_to_windows_and_leaves_focus_where_it_was(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = on_windows(cx, &stack);
    ui.input("half");

    let pressed = press(&mut ui, ON_THE_STRIP);
    send(&mut ui, up(ON_THE_STRIP));

    assert!(pressed.propagate, "Windows hears the press, and moves the window");
    assert!(pressed.default_prevented, "the window isn't focused for it");
    ui.input(" typed");
    assert_eq!(ui.composer_text(), "half typed");
}

/// The search hides the strip from the pointer, so Windows takes a press there as an ordinary one in the window, and
/// the strip, which would stop it as it does a press that a control took, never hears it.
#[gpui_kit::test]
fn on_windows_a_press_on_homes_search_never_reaches_the_strip(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = on_windows(cx, &stack);
    ui.click("home");
    ui.wait("Home", |ui| ui.has("home-page"));
    let on = center(&mut ui, "home-search");

    let pressed = press(&mut ui, on);

    assert!(pressed.propagate, "nothing stopped it");
    assert!(pressed.default_prevented, "the search took it");
}

/// On Windows GPUI doesn't capture the mouse for a press on the strip, so letting go of a button there off the
/// window loses the release, and the button would go on waiting for it. The next move finds the button up and lets go
/// where the pointer is, so the button isn't clicked by a release over it later.
#[gpui_kit::test]
fn on_windows_a_press_on_a_button_let_go_off_the_window_is_let_go(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = on_windows(cx, &stack);
    let on = center(&mut ui, "toggle-diff");
    assert!(!press(&mut ui, on).propagate, "the press is the button's");
    ui.frame();
    ui.cx.update(|cx| lost_release::the_left_button_is(Button::Up, cx));

    // Back over the window from off it, then over the button.
    send(&mut ui, MouseMoveEvent { position: point(on.x, px(300.)), pressed_button: None, ..Default::default() });
    send(&mut ui, MouseMoveEvent { position: on, pressed_button: None, ..Default::default() });
    send(&mut ui, up(on));

    assert!(ui.changes_open(), "the button wasn't clicked");
}

/// A new workspace, open in a window that goes by Windows' title bar, as it does on Windows.
fn on_windows<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    cx.update(|cx| the_title_bar_is(TitleBar::Windows, cx));
    in_a_new_workspace(cx, stack)
}

fn center(ui: &mut Ui<'_>, id: impl Into<ElementId>) -> Point<Pixels> {
    let id = id.into();
    ui.frame();
    ui.cx.update_window(ui.handle, |_, window, _| window.find(id).bounds().center()).unwrap()
}

/// Presses the left button at `position`, and says what the window did with the press, which is what GPUI's Windows
/// backend goes by: a press that propagates goes on to Windows.
fn press(ui: &mut Ui<'_>, position: Point<Pixels>) -> DispatchEventResult {
    let down = MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() };
    ui.cx.update_window(ui.handle, |_, window, cx| window.dispatch_event(down.to_platform_input(), cx)).unwrap()
}

fn up(position: Point<Pixels>) -> MouseUpEvent {
    MouseUpEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }
}

/// Sends the window a pointer event, and lets it draw.
fn send(ui: &mut Ui<'_>, event: impl InputEvent) {
    ui.cx.update_window(ui.handle, |_, window, cx| window.dispatch_event(event.to_platform_input(), cx)).unwrap();
    ui.frame();
}
