//! Settings' search, which finds a setting by what it's called or what it does.

#![cfg(test)]

use gpui_kit::{
    AppContext as _, InputEvent as _, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point, TestAppContext, point,
    px,
};

use super::{KEEP_AWAKE, open_settings, settings_showing, typing};
use crate::support::{Stack, Ui};

const THINKING: &str = "Always expand thinking";

/// Opens a workspace, then Settings over it.
fn in_settings<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let mut ui = typing(cx, stack, "");
    open_settings(&mut ui);
    ui
}

/// Whether each setting is left: always expanding thinking, then keeping the computer awake.
fn left(ui: &mut Ui<'_>) -> (bool, bool) {
    (!ui.labelled(THINKING).is_empty(), !ui.labelled(KEEP_AWAKE).is_empty())
}

/// Types into the search, with the cursor put there as Cmd-F does.
fn search_for(ui: &mut Ui<'_>, typed: &str) {
    ui.press("secondary-f");
    ui.input(typed);
    ui.frame();
}

#[gpui_kit::test]
fn a_setting_is_found_by_what_its_called(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_settings(cx, &stack);
    assert_eq!(left(&mut ui), (true, true));

    ui.press("tab");
    ui.input("awake");
    ui.frame();

    assert_eq!(left(&mut ui), (false, true), "only what matches is left");
}

#[gpui_kit::test]
fn cmd_f_puts_the_cursor_in_the_search_with_what_was_typed_selected(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_settings(cx, &stack);
    search_for(&mut ui, "awake");
    assert_eq!(left(&mut ui), (false, true));

    search_for(&mut ui, "thinking");

    assert_eq!(left(&mut ui), (true, false), "what's typed takes the place of what was");
}

/// Whether pressing `key` goes back from Settings with the cursor in the search and something typed there.
fn goes_back_from_the_search(cx: &mut TestAppContext, key: &str) -> bool {
    let stack = Stack::new();
    let mut ui = in_settings(cx, &stack);
    search_for(&mut ui, "awake");

    ui.press(key);

    !settings_showing(&mut ui)
}

#[gpui_kit::test]
fn escape_goes_back_from_the_search_too(cx: &mut TestAppContext) {
    assert!(goes_back_from_the_search(cx, "escape"));
}

#[gpui_kit::test]
fn cmd_w_goes_back_from_the_search_too(cx: &mut TestAppContext) {
    assert!(goes_back_from_the_search(cx, "secondary-w"));
}

#[gpui_kit::test]
fn settings_opens_again_with_nothing_in_the_search(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_settings(cx, &stack);
    search_for(&mut ui, "awake");
    ui.click("settings-back");
    assert!(!settings_showing(&mut ui));

    open_settings(&mut ui);

    assert_eq!(left(&mut ui), (true, true));
}

#[gpui_kit::test]
fn a_search_that_matches_nothing_says_so_and_leaves_no_page(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_settings(cx, &stack);
    let general = ui.labelled("General");
    assert_eq!(general.len(), 1, "the page's row");
    assert_eq!(general[0].selected(), Some(true), "it's the page showing");

    search_for(&mut ui, "nothing like it");

    assert!(ui.has("settings-nothing-found"));
    assert!(ui.labelled("General").is_empty(), "the page's row has gone too");
    assert_eq!(left(&mut ui), (false, false));
}

#[gpui_kit::test]
fn the_page_shows_again_once_the_search_no_longer_hides_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_settings(cx, &stack);
    search_for(&mut ui, "nothing like it");

    // Cmd-F selects what's typed on every platform, where the kit's own key for that is only Cmd-A on a Mac.
    ui.press("secondary-f");
    ui.press("backspace");

    assert!(!ui.has("settings-nothing-found"));
    assert_eq!(left(&mut ui), (true, true));
}

/// The strip along the top stands in for a title bar in Settings too, so pressing it leaves the cursor where it was.
#[gpui_kit::test]
fn a_click_on_the_strip_leaves_the_cursor_in_the_search(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_settings(cx, &stack);
    search_for(&mut ui, "awake");

    click_at(&mut ui, point(px(700.), px(20.)));
    ui.input(" nothing like it");
    ui.frame();

    assert_eq!(left(&mut ui), (false, false), "what's typed after it goes on in the search");
}

/// A press on the page below the strip that nothing there takes puts the cursor in no field, and keys still reach
/// Settings.
#[gpui_kit::test]
fn a_click_on_the_page_takes_the_cursor_out_of_the_search(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = in_settings(cx, &stack);
    search_for(&mut ui, "awake");

    click_at(&mut ui, point(px(700.), px(300.)));
    ui.input(" nothing like it");
    ui.frame();

    assert_eq!(left(&mut ui), (false, true), "what's typed after it isn't in the search");
    ui.press("escape");
    assert!(!settings_showing(&mut ui), "Escape still goes back");
}

/// Presses and lets go of the left button at `position`, without moving, so it's no drag.
fn click_at(ui: &mut Ui<'_>, position: Point<Pixels>) {
    let down = MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() };
    let up = MouseUpEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() };
    ui.cx
        .update_window(ui.handle, |_, window, cx| {
            window.dispatch_event(down.to_platform_input(), cx);
            window.dispatch_event(up.to_platform_input(), cx);
        })
        .unwrap();
    ui.frame();
}
