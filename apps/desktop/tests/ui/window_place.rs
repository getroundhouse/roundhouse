//! The window opening where it was left.

#![cfg(test)]

use std::path::PathBuf;

use gpui_kit::{App, AppContext as _, Pixels, Size, TestAppContext, Window, px, size};

use crate::support::{Stack, Ui};

/// Larger than the window opens until it has been left somewhere, as when it fills a display.
const FILLING: Size<Pixels> = size(px(1728.), px(1079.));

fn window_file(stack: &Stack) -> PathBuf {
    stack.hub.paths.data_dir.join("window.json")
}

fn window_size(ui: &mut Ui<'_>) -> Size<Pixels> {
    ui.cx.update_window(ui.handle, |_, window, _| window.bounds().size).unwrap()
}

/// The app, with its window left filling a display, and that saved.
fn left_filling<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let mut ui = Ui::open(cx, stack);
    assert_ne!(window_size(&mut ui), FILLING);
    ui.cx.simulate_window_resize(ui.handle, size(px(1500.), px(900.)));
    ui.cx.simulate_window_resize(ui.handle, FILLING);
    ui.cx.executor().advance_clock(desktop::WINDOW_SETTLES_AFTER);
    ui.wait("where the window is to be saved", |_| saved(stack).is_some_and(|saved| saved["width"] == 1728.0));
    ui
}

/// What's saved of where the window is.
fn saved(stack: &Stack) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(window_file(stack)).ok()?).ok()
}

/// Opens the window again and says how large it opened. On the same app, since each test app has a display of its
/// own, and a window whose display is gone opens in its usual place. It's closed again as it's measured, so it isn't
/// there to start a new hub when the test stops this one.
fn opened_again(ui: &mut Ui<'_>, stack: &Stack, opens: desktop::Opens) -> Size<Pixels> {
    let (hub, checkout) = (stack.hub.clone(), stack.checkout.clone());
    let (again, _) = ui.cx.update(|cx| desktop::open_main_window(hub, checkout, opens, cx)).unwrap();
    let measure = |_, window: &mut Window, _: &mut App| {
        window.remove_window();
        window.bounds().size
    };
    ui.cx.update_window(again, measure).unwrap()
}

#[gpui_kit::test]
fn the_window_opens_at_the_size_it_was_left_at(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = left_filling(cx, &stack);

    assert_eq!(opened_again(&mut ui, &stack, desktop::Opens::WhereItWasLeft), FILLING);
}

#[gpui_kit::test]
fn a_window_opened_for_a_picture_opens_in_its_usual_place_wherever_it_was_left(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = left_filling(cx, &stack);

    assert_eq!(opened_again(&mut ui, &stack, desktop::Opens::InItsUsualPlace), size(px(1320.), px(860.)));
}

#[gpui_kit::test]
fn where_the_window_is_as_the_app_quits_is_saved(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);
    let display = ui.cx.update(|cx| cx.primary_display().unwrap().uuid().unwrap().to_string());

    // Straight after, before the window has held still for long enough to be saved by itself.
    ui.cx.simulate_window_resize(ui.handle, FILLING);
    let at = ui.cx.update_window(ui.handle, |_, window, _| window.bounds().origin).unwrap();
    ui.cx.update(gpui_kit::App::shutdown);

    let saved = saved(&stack).unwrap();
    assert_eq!(saved["display"], display);
    assert_ne!(at.x, at.y, "or one saved as the other wouldn't show");
    assert_eq!((saved["x"].as_f64(), saved["y"].as_f64()), (Some(f64::from(at.x)), Some(f64::from(at.y))));
    assert_eq!((saved["width"].as_f64(), saved["height"].as_f64()), (Some(1728.), Some(1079.)));
    assert_eq!(saved["mode"], "windowed");
}

/// Closing the window quits the app, and by then the window can't be asked where it is.
#[gpui_kit::test]
fn a_window_that_changed_without_saying_and_is_then_closed_is_saved_as_it_was(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let ui = Ui::open(cx, &stack);
    ui.cx.simulate_window_resize(ui.handle, size(px(1500.), px(900.)));
    // With nothing said, as when a window has finished leaving full screen.
    ui.cx.update_window(ui.handle, |_, window, _| window.resize(FILLING)).unwrap();

    gpui_kit::VisualTestContext::from_window(ui.handle, ui.cx).simulate_close();
    ui.cx.update_window(ui.handle, |_, window, _| window.remove_window()).unwrap();
    ui.cx.update(gpui_kit::App::shutdown);

    assert_eq!(saved(&stack).unwrap()["width"], 1728.0);
}

#[gpui_kit::test]
fn a_closed_window_isnt_saved_as_it_was_before_it_was_last_saved(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    ui.cx.simulate_window_resize(ui.handle, size(px(1500.), px(900.)));
    ui.cx.update_window(ui.handle, |_, window, _| window.resize(FILLING)).unwrap();
    ui.cx.executor().advance_clock(desktop::WINDOW_SETTLES_AFTER);
    ui.wait("where the window is to be saved", |_| saved(&stack).is_some_and(|saved| saved["width"] == 1728.0));

    // Gone without being asked to close, so the last look at it was as it was saved.
    ui.cx.update_window(ui.handle, |_, window, _| window.remove_window()).unwrap();
    ui.cx.update(gpui_kit::App::shutdown);

    assert_eq!(saved(&stack).unwrap()["width"], 1728.0);
}

#[gpui_kit::test]
fn a_window_that_never_moved_saves_nothing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);

    ui.send_to_back();
    ui.bring_to_front();
    ui.cx.executor().advance_clock(desktop::WINDOW_SETTLES_AFTER);
    ui.frame();
    ui.cx.update(gpui_kit::App::shutdown);

    assert!(!window_file(&stack).exists());
}

#[gpui_kit::test]
fn a_window_left_in_full_screen_that_has_since_left_it_is_saved_as_a_window(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let display = cx.update(|cx| cx.primary_display().unwrap().uuid().unwrap());
    let left =
        format!(r#"{{"display": "{display}", "x": 0, "y": 1, "width": 1728, "height": 1079, "mode": "full_screen"}}"#);
    std::fs::create_dir_all(window_file(&stack).parent().unwrap()).unwrap();
    std::fs::write(window_file(&stack), left).unwrap();
    let mut ui = Ui::open(cx, &stack);
    assert_eq!(window_size(&mut ui), FILLING);

    // A test's window never says it's in full screen, so it's as one that has left it, back where it opened. GPUI
    // says a window has moved whenever it comes to the front.
    ui.send_to_back();
    ui.bring_to_front();
    ui.cx.executor().advance_clock(desktop::WINDOW_SETTLES_AFTER);

    ui.wait("the window to be saved as one that isn't in full screen", |_| {
        std::fs::read_to_string(window_file(&stack)).is_ok_and(|saved| saved.contains(r#""mode": "windowed""#))
    });
}
