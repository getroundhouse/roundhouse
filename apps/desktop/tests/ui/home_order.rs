//! The order of Home's workspaces, which holds still while someone is using the list and slides as it changes, as the
//! sidebar's does. Both lists are drawn by `desktop::reorder`, whose rules its unit tests pin and `sidebar_order.rs`
//! shows through the sidebar, so here one walk shows Home wired to each, and a search, which only Home has, is shown
//! on its own.

#![cfg(test)]

use std::time::Duration;

use client_core::fake_hub::World;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::{SessionId, WorkspaceId};

use crate::support::{
    Stack, Ui, home_row, open_on_the_world, order, sidebar_row, three_workspaces, top, with_a_chat, worked_in,
};

/// A window with three workspaces in one repository, made in the order they're returned, each with its chat, and Home
/// open over the last.
fn home_over_three<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, [WorkspaceId; 3], [SessionId; 3]) {
    let (mut ui, workspaces, chats) = three_workspaces(cx, stack);
    ui.click("home");
    ui.wait("Home", |ui| ui.has("home-page"));
    (ui, workspaces, chats)
}

/// Each step has another workspace worked in, which comes to the top: first with the pointer on the list, which holds
/// the order still until it's rested, then with motion allowed, so it slides there, and then while Settings hides
/// Home, which shows again with the rows in their places at once.
#[gpui_kit::test]
fn homes_order_holds_for_the_pointer_slides_as_it_changes_and_is_in_place_at_once_after_settings(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [first_chat, second_chat, third_chat]) = home_over_three(cx, &stack);
    let made = [first.clone(), second.clone(), third.clone()];

    held_for_the_pointer(&mut ui, &stack, &made, &second_chat);
    slid(&mut ui, &stack, &made, &third_chat);
    in_place_after_settings(&mut ui, &stack, &made, &first_chat);
}

/// With the pointer on the list, `chat`'s workspace, the second made, is worked in, and the order holds still until
/// the pointer has rested.
fn held_for_the_pointer(ui: &mut Ui, stack: &Stack, made: &[WorkspaceId; 3], chat: &SessionId) {
    let [first, second, third] = made.clone();
    ui.hover(home_row(&first));

    worked_in(ui, stack, chat);
    // With nothing typed into, so the search's cursor doesn't blink and draw the window again meanwhile, and the order
    // shows only if the end of the hold draws it.
    ui.cx.update_window(ui.handle, |_, window, cx| window.blur(cx)).unwrap();
    ui.frame();

    let held = [third.clone(), second.clone(), first.clone()];
    assert_eq!(order(ui, home_row, made), held, "the pointer holds it, so it can't move from a click");
    let sorted = [second, third, first];
    assert_eq!(order(ui, sidebar_row, made), sorted, "the sidebar's, which the pointer isn't on, doesn't hold");
    let a_moment = Duration::from_millis(100);
    ui.cx.executor().advance_clock(desktop::ORDER_HOLDS_FOR.checked_sub(a_moment).unwrap());
    ui.frame();
    assert_eq!(order(ui, home_row, made), held, "the pointer holds it, not quite rested yet");
    ui.cx.executor().advance_clock(a_moment);
    ui.frame();
    assert_eq!(order(ui, home_row, made), sorted, "the pointer has rested, though it's still there");
}

/// With motion allowed, `chat`'s workspace, the third made, second in the list, is worked in, and slides to the top.
fn slid(ui: &mut Ui, stack: &Stack, made: &[WorkspaceId; 3], chat: &SessionId) {
    let [first, second, third] = made.clone();
    let (was, goes) = (top(ui, home_row(&third)), top(ui, home_row(&second)));
    assert!(goes < was, "the one last worked in is at the top: {goes:?}, {was:?}");
    ui.allow_motion();

    worked_in(ui, stack, chat);

    assert_eq!(top(ui, home_row(&third)), was, "the row slides from where it was, since no time has passed");
    ui.play(look::motion::PANEL / 2);
    let halfway = top(ui, home_row(&third));
    assert!(goes < halfway && halfway < was, "the row sliding, at {halfway:?}, isn't between {goes:?} and {was:?}");
    ui.play(look::motion::PANEL);
    assert_eq!(top(ui, home_row(&third)), goes, "the row slid to the top");
    assert_eq!(ui.play(look::motion::PANEL), 0, "and nothing asks for another frame once the row has slid");
    assert_eq!(order(ui, home_row, made), [third, second, first], "the order the row slid into");
}

/// With the pointer on the list, Settings is opened over Home, `chat`'s workspace, the first made, at the bottom, is
/// worked in, and Settings goes.
fn in_place_after_settings(ui: &mut Ui, stack: &Stack, made: &[WorkspaceId; 3], chat: &SessionId) {
    let [first, second, third] = made.clone();
    let goes = top(ui, home_row(&third));
    ui.hover(home_row(&first));
    // From the menu bar, since a key would end the hold by itself.
    ui.cx.update(|cx| cx.dispatch_action(&desktop::OpenSettings));
    ui.frame();

    worked_in(ui, stack, chat);
    ui.press("escape");

    assert_eq!(top(ui, home_row(&first)), goes, "after Settings, rather than sliding from where it was before them");
    let shown = order(ui, home_row, made);
    assert_eq!(shown, [first, third, second], "after Settings, since Home has heard nothing of the pointer since");
}

/// A search asks nothing of a hub, so a fake one will do, holding three workspaces each with a chat, made one after
/// another, so the last made is the last worked in.
#[gpui_kit::test]
fn the_rows_a_search_keeps_are_in_their_places_at_once_rather_than_sliding_there(cx: &mut TestAppContext) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let first = with_a_chat(&mut world, &repo, "ledger");
    for name in ["points", "sidings"] {
        with_a_chat(&mut world, &repo, name);
    }
    let (_hub, mut ui) = open_on_the_world(cx, world);
    ui.wait("a workspace", |ui| ui.pane().is_some());
    ui.click("home");
    ui.wait("Home", |ui| ui.has("home-page"));
    let was = top(&mut ui, home_row(&first));
    let branch = ui.view().records().workspaces().iter().find(|w| w.id == first).unwrap().branch.clone();
    ui.allow_motion();

    ui.input(branch.as_str());

    let at_once = top(&mut ui, home_row(&first));
    ui.play(look::motion::PANEL);
    let at_rest = top(&mut ui, home_row(&first));
    assert!(at_rest < was, "the oldest, at the bottom, is the only one left, at the top: {at_rest:?}, {was:?}");
    assert_eq!(at_once, at_rest);
}
