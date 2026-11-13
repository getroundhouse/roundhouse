//! The menus as the app draws them off the Mac: from a button in the strip's top left corner, or F10, as one of the
//! kit's popup menus with each menu a submenu of it. What each menu holds is in `mod.rs`, read from what GPUI keeps,
//! which is what's drawn.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, point, px};

use super::with_a_repository;
use crate::sidebars::show_an_error;
use crate::support::{MENU, Stack, Ui};

/// Whether the menus are open, with a submenu or not, each of which is a menu of its own.
fn open(ui: &mut Ui<'_>) -> bool {
    ui.frame();
    ui.cx.update_window(ui.handle, |_, window, _| !window.find_all(MENU).is_empty()).unwrap()
}

/// Chooses an item from the menus, from the button, as a hand does.
fn choose(ui: &mut Ui<'_>, menu: &str, item: &str) {
    ui.click("menus");
    ui.click_menu_item(menu);
    ui.click_menu_item(item);
}

#[gpui_kit::test]
fn the_button_opens_the_menus_and_an_item_does_what_its_key_does(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);
    ui.new_workspace();
    let before = ui.open_chats().len();

    ui.click("menus");
    assert!(open(&mut ui));
    ui.click_menu_item("File");
    ui.click_menu_item("New Chat");

    ui.wait("the new chat", |ui| ui.open_chats().len() == before + 1);
    assert!(!open(&mut ui), "choosing an item closes the menus");
    ui.press("secondary-t");
    ui.wait("another, from the key", |ui| ui.open_chats().len() == before + 2);
}

/// They act on what had focus before they opened, as the menu bar's do, and give it back.
#[gpui_kit::test]
fn the_edit_menu_edits_the_composer_and_leaves_it_focused(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);
    ui.new_workspace();
    ui.input("say:hello");
    ui.press("secondary-a");

    choose(&mut ui, "Edit", "Cut");

    assert_eq!(ui.composer_text(), "");
    ui.input("typed");
    assert_eq!(ui.composer_text(), "typed", "the composer has the keys again");
}

/// As the menu bar greys out an item where nothing would take it, so choosing it does nothing, and the menus stay open.
#[gpui_kit::test]
fn an_item_that_cant_act_is_greyed_out(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);

    choose(&mut ui, "File", "New Chat");

    assert!(open(&mut ui), "with no workspace to open a chat in, it can't be chosen");
    assert!(ui.pane().is_none());
}

/// As F10 opens a Windows or Linux app's menus. Escape closes them, and F10 again does too.
#[gpui_kit::test]
fn f10_opens_the_menus_and_escape_or_f10_closes_them(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);
    ui.new_workspace();
    ui.input("half");

    ui.press("f10");
    assert!(open(&mut ui));
    ui.press("escape");
    assert!(!open(&mut ui));
    ui.press("f10");
    ui.press("f10");

    assert!(!open(&mut ui));
    ui.input(" typed");
    assert_eq!(ui.composer_text(), "half typed", "focus is back where it was");
}

#[gpui_kit::test]
fn pressing_the_button_again_closes_the_menus(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);
    ui.click("menus");

    ui.click("menus");

    assert!(!open(&mut ui));
}

/// It's before the button that hides the sidebar, and stays put as that does, over Settings too, which has no sidebar
/// to hide.
#[gpui_kit::test]
fn the_button_stays_where_it_is(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);
    ui.new_workspace();
    let button = ui.bounds("menus");
    assert!(button.right() < ui.bounds("toggle-sidebar").left(), "it's first in the corner: {button:?}");

    ui.click("toggle-sidebar");
    assert_eq!(ui.bounds("menus"), button, "with the sidebar hidden");
    ui.press("secondary-,");

    ui.wait("Settings", |ui| ui.has("settings-back"));
    assert_eq!(ui.bounds("menus"), button, "over Settings");
}

/// Presses Tab until the button has focus, as someone without a pointer reaches it.
fn tab_to_the_button(ui: &mut Ui<'_>) {
    for _ in 0..20 {
        ui.press("tab");
        ui.frame();
        let focused = ui.cx.update_window(ui.handle, |_, window, _| window.find("menus").focused()).unwrap();
        if focused == Some(true) {
            return;
        }
    }
    panic!("Tab never reaches the button");
}

/// Enter on the button, once Tab has reached it, opens the menus, as a click does.
#[gpui_kit::test]
fn the_button_opens_the_menus_from_the_keyboard(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);
    tab_to_the_button(&mut ui);

    ui.press("enter");

    assert!(open(&mut ui));
}

/// What the app takes wherever focus is, such as About, isn't greyed out, though no view takes it.
#[gpui_kit::test]
fn about_can_be_chosen_from_the_menus(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);

    choose(&mut ui, "Help", "About Roundhouse");

    ui.wait("the alert", |ui| ui.cx.has_pending_prompt());
}

/// While the sidebar is hidden the button is over the strip, and over an error in it, which a press that opens the
/// menus leaves alone, as one on the sidebar's button does.
#[gpui_kit::test]
fn the_button_leaves_an_error_under_it_alone(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);
    ui.new_workspace();
    ui.click("toggle-sidebar");
    show_an_error(&mut ui);

    ui.click("menus");

    assert!(open(&mut ui));
    assert!(ui.view().error.is_some(), "the error is still there");
}

/// As a menu bar's menus close when you click away from them.
#[gpui_kit::test]
fn a_click_anywhere_else_closes_the_menus(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = with_a_repository(cx, &stack);
    ui.click("menus");
    let button = ui.bounds("menus");

    ui.click_at(button.center() + point(px(600.), px(300.)));

    assert!(!open(&mut ui));
}
