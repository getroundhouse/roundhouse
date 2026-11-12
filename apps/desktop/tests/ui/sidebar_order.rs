//! The order of the sidebar's workspaces, which its heading chooses, and the rows sliding as it changes.

#![cfg(test)]

use std::time::Duration;

use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{AppContext as _, ElementId, Pixels, SharedString, TestAppContext};
use protocol::model::{Seq, Session, SessionId, WorkspaceId};

use crate::support::{Stack, Ui};

fn row(id: &WorkspaceId) -> ElementId {
    ElementId::Name(SharedString::from(format!("workspace-{id}")))
}

/// How far down the window the last frame drew a workspace's row.
fn top(ui: &mut Ui, id: &WorkspaceId) -> Pixels {
    ui.frame();
    let row = row(id);
    ui.cx.update_window(ui.handle, |_, window, _| window.find(row).bounds().top()).unwrap()
}

/// The rows from the top of the sidebar down.
fn order<const N: usize>(ui: &mut Ui, workspaces: &[WorkspaceId; N]) -> Vec<WorkspaceId> {
    let mut drawn: Vec<(Pixels, WorkspaceId)> = workspaces.iter().map(|id| (top(ui, id), id.clone())).collect();
    drawn.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    drawn.into_iter().map(|(_, id)| id).collect()
}

/// Whether the last frame drew a menu with this item in it.
fn menu_shows(ui: &mut Ui, label: &str) -> bool {
    ui.frame();
    let menu = ElementId::Name("popup-menu".into());
    let found = |window: &gpui_kit::Window| window.find_all_by_label(label).iter().any(|f| f.path().contains(&menu));
    ui.cx.update_window(ui.handle, |_, window, _| found(window)).unwrap()
}

/// Chooses an order from the heading's menu, whose button shows while the pointer is on the heading.
fn sort_by(ui: &mut Ui, order: &str) {
    ui.hover("workspaces-heading");
    ui.click("sort-workspaces");
    ui.click_menu_item(order);
}

/// Sends `chat` its first prompt, as the person would, and waits until the app has heard its agent reply, so nothing
/// in the window is still at work, and heard from its workspace's record that it was worked in then.
fn worked_in(ui: &mut Ui, stack: &Stack, chat: &SessionId) {
    stack.client().session_prompt(chat, "say:done", None).unwrap();
    let replied = |c: &Session| c.user_active_at_ms.is_some() && c.reply_seq != Seq::default() && !c.status.in_turn();
    let caught_up = |ui: &mut Ui| {
        let view = ui.view();
        let Some(c) = view.sessions.values().flatten().find(|c| &c.id == chat && replied(c)) else { return false };
        view.workspaces.iter().any(|w| w.id == c.workspace_id && w.user_active_at_ms == c.user_active_at_ms)
    };
    ui.wait("the reply", caught_up);
}

/// A window with three workspaces in one repository, made in the order they're returned, each with its chat.
fn three_workspaces<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, [WorkspaceId; 3], [SessionId; 3]) {
    let mut ui = Ui::open_with_a_repository(cx, stack);
    let mut made = Vec::new();
    for _ in 0..3 {
        let workspace = ui.new_workspace();
        made.push((workspace, ui.active_chat()));
    }
    let (workspaces, chats): (Vec<_>, Vec<_>) = made.into_iter().unzip();
    (ui, workspaces.try_into().unwrap(), chats.try_into().unwrap())
}

#[gpui_kit::test]
fn the_sidebar_starts_last_active_first_and_the_heading_chooses_another_and_the_keys_go_in_its_order(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [chat, ..]) = three_workspaces(cx, &stack);
    assert!(!ui.visible("sort-workspaces"), "the button shows only while the pointer is on the heading");
    let made = [first.clone(), second.clone(), third.clone()];

    worked_in(&mut ui, &stack, &chat);

    let sorted = [first.clone(), third.clone(), second.clone()];
    assert_eq!(order(&mut ui, &made), sorted, "the one worked in, then the newest, which nobody has worked in");
    ui.click(row(&first));
    ui.press("cmd-]");
    assert_eq!(ui.view().selected.as_ref(), Some(&third), "the next one down the sidebar");

    sort_by(&mut ui, "Oldest first");

    assert_eq!(order(&mut ui, &made), made);
    ui.press("cmd-[");
    assert_eq!(ui.view().selected.as_ref(), Some(&second), "the one above it now");
}

#[gpui_kit::test]
fn a_row_slides_to_its_new_place_and_comes_to_rest_there(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [_, chat, _]) = three_workspaces(cx, &stack);
    let (was, goes) = (top(&mut ui, &second), top(&mut ui, &third));
    assert!(goes < was, "the newest is at the top: {goes:?}, {was:?}");
    ui.allow_motion();

    worked_in(&mut ui, &stack, &chat);

    assert_eq!(top(&mut ui, &second), was, "it starts from where it was, since no time has passed");
    ui.play(look::motion::PANEL / 2);
    let halfway = top(&mut ui, &second);
    assert!(goes < halfway && halfway < was, "{halfway:?} isn't between {goes:?} and {was:?}");
    ui.play(look::motion::PANEL);
    assert_eq!(top(&mut ui, &second), goes);
    assert_eq!(ui.play(look::motion::PANEL), 0, "and nothing asks for another frame");
    assert_eq!(order(&mut ui, &[first.clone(), second.clone(), third.clone()]), [second, third, first]);
}

#[gpui_kit::test]
fn the_order_holds_still_while_the_pointer_is_on_the_list_and_changes_once_it_leaves(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [_, chat, _]) = three_workspaces(cx, &stack);
    let made = [first.clone(), second.clone(), third.clone()];
    ui.hover(row(&first));

    worked_in(&mut ui, &stack, &chat);

    assert_eq!(order(&mut ui, &made), [third.clone(), second.clone(), first.clone()], "so it can't move from a click");
    ui.press("cmd-]");
    assert_eq!(ui.view().selected.as_ref(), Some(&second), "the keys go in the order shown");
    ui.hover("send");
    assert_eq!(order(&mut ui, &made), [second, third, first]);
}

#[gpui_kit::test]
fn the_order_changes_once_the_pointer_has_rested_on_the_list_for_a_moment(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [_, chat, _]) = three_workspaces(cx, &stack);
    let made = [first.clone(), second.clone(), third.clone()];
    ui.hover(row(&first));

    worked_in(&mut ui, &stack, &chat);
    // With nothing typed into, so no cursor blinks and draws the window again meanwhile, and the order shows only if
    // the end of the hold draws it.
    ui.cx.update_window(ui.handle, |_, window, cx| window.blur(cx)).unwrap();
    ui.frame();

    let a_moment = Duration::from_millis(100);
    ui.cx.executor().advance_clock(desktop::SIDEBAR_ORDER_HOLDS_FOR.checked_sub(a_moment).unwrap());
    ui.frame();
    assert_eq!(order(&mut ui, &made), [third.clone(), second.clone(), first.clone()], "not quite yet");
    ui.cx.executor().advance_clock(a_moment);
    ui.frame();
    assert_eq!(order(&mut ui, &made), [second, third, first], "with the pointer still there");
}

#[gpui_kit::test]
fn the_order_doesnt_hold_for_a_pointer_that_was_on_the_list_before_settings_hid_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [_, chat, _]) = three_workspaces(cx, &stack);
    let made = [first.clone(), second.clone(), third.clone()];
    ui.hover(row(&first));
    // From the menu bar, since a key would end the hold by itself, as the next test says.
    ui.cx.update(|cx| cx.dispatch_action(&desktop::OpenSettings));
    ui.frame();

    worked_in(&mut ui, &stack, &chat);
    ui.press("escape");

    assert_eq!(order(&mut ui, &made), [second, third, first], "since it's heard nothing of the pointer since");
}

#[gpui_kit::test]
fn the_order_doesnt_hold_once_a_key_is_pressed_until_the_pointer_moves_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [_, chat, _]) = three_workspaces(cx, &stack);
    let made = [first.clone(), second.clone(), third.clone()];
    ui.hover(row(&first));
    ui.press("a");

    worked_in(&mut ui, &stack, &chat);

    assert_eq!(order(&mut ui, &made), [second, third, first], "so it goes on changing while someone types");
}

#[gpui_kit::test]
fn the_order_holds_still_while_a_rows_menu_is_open_wherever_the_pointer_goes(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [_, chat, _]) = three_workspaces(cx, &stack);
    let made = [first.clone(), second.clone(), third.clone()];
    ui.right_click(row(&second));
    ui.hover("send");

    worked_in(&mut ui, &stack, &chat);

    assert_eq!(order(&mut ui, &made), [third.clone(), second.clone(), first.clone()], "or the menu would go");
    assert!(menu_shows(&mut ui, "Archive"), "the menu is still there");
    ui.press("escape");
    assert_eq!(order(&mut ui, &made), [second, third, first]);
}
