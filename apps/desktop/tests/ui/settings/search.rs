//! Settings' search, which finds a setting by what it's called or what it does. What a search leaves is
//! `settings::page`'s rule, which its unit tests pin, so this shows that what's typed reaches it, and where the cursor
//! and the keys go, on a fake hub, since a search asks the hub for nothing.

#![cfg(test)]

use gpui_kit::{
    AppContext as _, InputEvent as _, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point, TestAppContext, point,
    px,
};

use super::made_up::in_a_workspace;
use super::{KEEP_AWAKE, open_settings, settings_showing};
use crate::support::Ui;

const THINKING: &str = "Always expand thinking";

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

/// Types after whatever has the cursor.
fn type_after(ui: &mut Ui<'_>, typed: &str) {
    ui.input(typed);
    ui.frame();
}

/// One walk through one window, each step a case of what's typed reaching the search, or of where the cursor and the
/// keys go, each named in its assertion. Nothing here waits, since nothing goes to the hub.
#[gpui_kit::test]
fn the_search_leaves_what_matches_what_is_typed_there_and_the_keys_still_reach_settings(cx: &mut TestAppContext) {
    let (_hub, mut ui) = in_a_workspace(cx);
    open_settings(&mut ui);
    assert_eq!(left(&mut ui), (true, true), "with nothing typed, everything is left");
    let general = ui.labelled("General");
    assert_eq!(general.len(), 1, "the page's row");
    assert_eq!(general[0].selected(), Some(true), "it's the page showing");

    ui.press("tab");
    type_after(&mut ui, "awake");
    assert_eq!(left(&mut ui), (false, true), "Tab: what's typed after it finds a setting by what it's called");

    click_at(&mut ui, point(px(700.), px(300.)));
    type_after(&mut ui, " nothing like it");
    assert_eq!(left(&mut ui), (false, true), "a press on the page: what's typed after it isn't in the search");
    ui.press("escape");
    assert!(!settings_showing(&mut ui), "a press on the page: Escape still goes back");

    open_settings(&mut ui);
    assert_eq!(left(&mut ui), (true, true), "after Escape: Settings opens again with nothing in the search");
    search_for(&mut ui, "awake");
    assert_eq!(left(&mut ui), (false, true), "Cmd-F puts the cursor in the search");
    search_for(&mut ui, "thinking");
    assert_eq!(left(&mut ui), (true, false), "Cmd-F again: what's typed takes the place of what was");

    // The strip along the top stands in for a title bar in Settings too, so pressing it leaves the cursor where it
    // was.
    click_at(&mut ui, point(px(700.), px(20.)));
    type_after(&mut ui, " nothing like it");
    assert_eq!(left(&mut ui), (false, false), "a press on the strip: what's typed after it goes on in the search");
    assert!(ui.has("settings-nothing-found"), "a search that matches nothing says so");
    assert!(ui.labelled("General").is_empty(), "a search that matches nothing leaves no page's row either");

    // Cmd-F selects what's typed on every platform, where the kit's own key for that is only Cmd-A on a Mac.
    ui.press("secondary-f");
    ui.press("backspace");
    assert!(!ui.has("settings-nothing-found"), "with the search emptied, nothing-found goes");
    assert_eq!(left(&mut ui), (true, true), "with the search emptied, the page shows again");

    search_for(&mut ui, "awake");
    ui.click("settings-back");
    assert!(!settings_showing(&mut ui), "the button goes back from the search");
    open_settings(&mut ui);
    assert_eq!(left(&mut ui), (true, true), "after the button: Settings opens again with nothing in the search");

    for key in ["escape", "secondary-w"] {
        search_for(&mut ui, "awake");
        assert_eq!(left(&mut ui), (false, true), "{key}: the cursor is in the search, with something typed");
        ui.press(key);
        assert!(!settings_showing(&mut ui), "{key} goes back from the search too");
        open_settings(&mut ui);
    }
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
