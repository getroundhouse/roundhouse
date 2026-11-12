//! The order of Home's workspaces, which holds still while someone is using the list and slides as it changes, as the
//! sidebar's does.

#![cfg(test)]

use std::time::Duration;

use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::{SessionId, WorkspaceId};

use crate::support::{Stack, Ui, home_row, order, sidebar_row, three_workspaces, top, worked_in};

/// A window with three workspaces in one repository, made in the order they're returned, each with its chat, and Home
/// open over the last.
fn home_over_three<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, [WorkspaceId; 3], [SessionId; 3]) {
    let (mut ui, workspaces, chats) = three_workspaces(cx, stack);
    ui.click("home");
    ui.wait("Home", |ui| ui.has("home-page"));
    (ui, workspaces, chats)
}

#[gpui_kit::test]
fn a_row_in_home_slides_to_its_new_place_and_comes_to_rest_there(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [_, chat, _]) = home_over_three(cx, &stack);
    let (was, goes) = (top(&mut ui, home_row(&second)), top(&mut ui, home_row(&third)));
    assert!(goes < was, "the newest is at the top: {goes:?}, {was:?}");
    ui.allow_motion();

    worked_in(&mut ui, &stack, &chat);

    assert_eq!(top(&mut ui, home_row(&second)), was, "it starts from where it was, since no time has passed");
    ui.play(look::motion::PANEL / 2);
    let halfway = top(&mut ui, home_row(&second));
    assert!(goes < halfway && halfway < was, "{halfway:?} isn't between {goes:?} and {was:?}");
    ui.play(look::motion::PANEL);
    assert_eq!(top(&mut ui, home_row(&second)), goes);
    assert_eq!(ui.play(look::motion::PANEL), 0, "and nothing asks for another frame");
    assert_eq!(order(&mut ui, home_row, &[first.clone(), second.clone(), third.clone()]), [second, third, first]);
}

#[gpui_kit::test]
fn the_rows_a_search_keeps_are_in_their_places_at_once_rather_than_sliding_there(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, ..], _) = home_over_three(cx, &stack);
    let was = top(&mut ui, home_row(&first));
    let branch = ui.view().workspaces.iter().find(|w| w.id == first).unwrap().branch.clone();
    ui.allow_motion();

    ui.input(branch.as_str());

    let at_once = top(&mut ui, home_row(&first));
    ui.play(look::motion::PANEL);
    let at_rest = top(&mut ui, home_row(&first));
    assert!(at_rest < was, "the oldest, at the bottom, is the only one left, at the top: {at_rest:?}, {was:?}");
    assert_eq!(at_once, at_rest);
}

#[gpui_kit::test]
fn homes_order_holds_still_while_the_pointer_is_on_its_list_and_changes_once_it_has_rested(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [_, chat, _]) = home_over_three(cx, &stack);
    let made = [first.clone(), second.clone(), third.clone()];
    ui.hover(home_row(&first));

    worked_in(&mut ui, &stack, &chat);
    // With nothing typed into, so the search's cursor doesn't blink and draw the window again meanwhile, and the order
    // shows only if the end of the hold draws it.
    ui.cx.update_window(ui.handle, |_, window, cx| window.blur(cx)).unwrap();
    ui.frame();

    let held = [third.clone(), second.clone(), first.clone()];
    assert_eq!(order(&mut ui, home_row, &made), held, "so it can't move from a click");
    let sorted = [second, third, first];
    assert_eq!(order(&mut ui, sidebar_row, &made), sorted, "the sidebar's, which the pointer isn't on, doesn't hold");
    let a_moment = Duration::from_millis(100);
    ui.cx.executor().advance_clock(desktop::ORDER_HOLDS_FOR.checked_sub(a_moment).unwrap());
    ui.frame();
    assert_eq!(order(&mut ui, home_row, &made), held, "not quite yet");
    ui.cx.executor().advance_clock(a_moment);
    ui.frame();
    assert_eq!(order(&mut ui, home_row, &made), sorted, "with the pointer still there");
}

#[gpui_kit::test]
fn home_shown_again_has_its_rows_in_their_places_at_once_without_holding_them_for_the_pointer_before(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [_, chat, _]) = home_over_three(cx, &stack);
    let made = [first.clone(), second.clone(), third.clone()];
    let goes = top(&mut ui, home_row(&third));
    ui.allow_motion();
    ui.hover(home_row(&first));
    // From the menu bar, since a key would end the hold by itself.
    ui.cx.update(|cx| cx.dispatch_action(&desktop::OpenSettings));
    ui.frame();

    worked_in(&mut ui, &stack, &chat);
    ui.press("escape");

    assert_eq!(top(&mut ui, home_row(&second)), goes, "rather than sliding from where it was before Settings");
    assert_eq!(
        order(&mut ui, home_row, &made),
        [second, third, first],
        "since it's heard nothing of the pointer since"
    );
}
