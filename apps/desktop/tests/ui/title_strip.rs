//! The strip along the window's top, which stands in for a title bar. Moving the window is the platform's, so it's
//! checked by hand. The test window can't move, and panics if anything asks it to, so these check what the strip
//! leaves alone.

#![cfg(test)]

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, InputEvent as _, MouseButton, MouseDownEvent, MouseUpEvent, point, px};

use crate::support::{Stack, Ui};

#[gpui_kit::test]
fn a_click_on_the_strip_leaves_the_composer_focused(cx: &mut gpui_kit::TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    ui.input("half");

    // In the header, between the workspace's name and its buttons, where a title bar would be.
    let position = point(px(700.), px(20.));
    ui.cx
        .update_window(ui.handle, |_, window, cx| {
            let down = MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() };
            window.dispatch_event(down.to_platform_input(), cx);
            let up = MouseUpEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() };
            window.dispatch_event(up.to_platform_input(), cx);
        })
        .unwrap();
    ui.frame();

    ui.input(" typed");
    assert_eq!(ui.composer_text(), "half typed");
}

#[gpui_kit::test]
fn a_drag_from_a_button_in_the_strip_is_the_buttons(cx: &mut gpui_kit::TestAppContext) {
    let stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, &stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.new_workspace();
    let pane = ui.pane().unwrap();

    // Let go off the button, so it isn't a click either.
    let from = ui.cx.update_window(ui.handle, |_, window, _| window.find("toggle-diff").bounds().center()).unwrap();
    let to = point(from.x, from.y + px(200.));
    ui.cx.update_window(ui.handle, |_, window, cx| window.drag(from, to, cx)).unwrap();
    ui.frame();
    assert!(!ui.cx.read_entity(&pane, |p, _| p.diff_open));

    ui.click("toggle-diff");
    assert!(ui.cx.read_entity(&pane, |p, _| p.diff_open));
}
