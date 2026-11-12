//! Windows' minimize, maximize and close, which the app draws at the right end of the strip along the top, standing
//! in for Windows, where the tests mostly don't build. GPUI's Windows backend works each from a press the app lets
//! propagate, so these check what the app does with the press, and what's in the strip ends before them.

#![cfg(test)]

use desktop::app::{TitleBar, the_title_bar_is};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, DispatchEventResult, ElementId, InputEvent as _, MouseButton, MouseDownEvent, MouseUpEvent,
    Pixels, Point, TestAppContext, px, size,
};

use crate::support::{Stack, Ui};
use crate::tools::in_a_new_workspace;

/// How wide the three are together.
const BUTTONS: Pixels = px(138.);

const BUTTON_IDS: [&str; 3] = ["window-minimize", "window-maximize", "window-close"];

/// A pull request with a failed check, whose bar offers Fix checks.
const CHECK_FAILED: &str = r#"{"number":7,"url":"https://example.com/pull/7","title":"T","state":"OPEN",
    "isDraft":false,"statusCheckRollup":[
        {"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"FAILURE"}]}"#;

#[gpui_kit::test]
fn on_windows_a_press_on_a_window_button_goes_on_to_windows_and_nothing_in_the_app_hears_it(cx: &mut TestAppContext) {
    cx.update(|cx| the_title_bar_is(TitleBar::Windows, cx));
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);

    for id in BUTTON_IDS {
        let pressed = click(&mut ui, id);

        assert!(pressed.propagate, "Windows hears the press on {id}");
        assert!(!pressed.default_prevented, "nothing under {id} took it, the strip included");
    }
}

/// A dialog's backdrop fills the window over the app, and is a title bar to Windows, so the buttons are drawn over it.
#[gpui_kit::test]
fn on_windows_the_window_buttons_are_over_an_open_dialog(cx: &mut TestAppContext) {
    cx.update(|cx| the_title_bar_is(TitleBar::Windows, cx));
    let stack = Stack::new();
    let mut ui = in_a_new_workspace(cx, &stack);
    ui.open_new_workspace();

    for id in BUTTON_IDS {
        let pressed = click(&mut ui, id);

        assert!(pressed.propagate && !pressed.default_prevented, "nothing in the dialog heard the press on {id}");
    }
    assert!(ui.has_dialog(), "and the dialog stays open");
}

/// At its narrowest, the bar at the top of the sidebar of changes has less room beside the buttons than the number
/// and Fix checks need, so the number goes, rather than the action running under them.
#[gpui_kit::test]
fn on_windows_the_pull_requests_bar_at_its_narrowest_ends_before_the_window_buttons(cx: &mut TestAppContext) {
    cx.update(|cx| the_title_bar_is(TitleBar::Windows, cx));
    let stack = Stack::new();
    let gh = stack.start_hub_with_stand_in_gh();
    gh.report(CHECK_FAILED);
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    // Narrow enough that the sidebar of changes is at its narrowest.
    ui.cx.simulate_window_resize(ui.handle, size(px(900.), px(800.)));
    ui.wait("the bar to offer Fix checks", |ui| ui.shows_label("pull-request-bar", "Fix checks"));

    let across = ui.cx.update_window(ui.handle, |_, window, _| window.viewport_size().width).unwrap();
    let ends = ui.cx.update_window(ui.handle, |_, window, _| window.find("pull-request-action").bounds().right());

    let ends = ends.unwrap();
    assert!(ends <= across - BUTTONS, "the action ends at {ends:?}, past where the buttons start");
    assert!(!ui.visible("pull-request"), "the number went to make room");
}

/// Presses the left button on `id` and lets it go, and says what the window did with the press, which is what GPUI's
/// Windows backend goes by: a press that propagates goes on to Windows.
fn click(ui: &mut Ui<'_>, id: &'static str) -> DispatchEventResult {
    ui.frame();
    let at: Point<Pixels> =
        ui.cx.update_window(ui.handle, |_, window, _| window.find(ElementId::from(id)).bounds().center()).unwrap();
    let down = MouseDownEvent { button: MouseButton::Left, position: at, click_count: 1, ..Default::default() };
    let up = MouseUpEvent { button: MouseButton::Left, position: at, click_count: 1, ..Default::default() };
    let pressed =
        ui.cx.update_window(ui.handle, |_, window, cx| window.dispatch_event(down.to_platform_input(), cx)).unwrap();
    ui.cx.update_window(ui.handle, |_, window, cx| window.dispatch_event(up.to_platform_input(), cx)).unwrap();
    ui.frame();
    pressed
}
