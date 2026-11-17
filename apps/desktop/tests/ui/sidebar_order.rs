//! The order of the sidebar's workspaces, which its heading chooses, and the rows sliding as it changes. The order
//! chosen is the app's own, kept in its settings, so a test of only that opens on a fake hub. One of the order changing
//! as the hub hears a workspace was worked in or made, which holds still while someone is using the list, opens on a
//! real one.

#![cfg(test)]

use std::time::Duration;

use client_core::fake_hub::{NOW_MS, World};
use gpui_kit::test::TestQueryExt as _;
use gpui_kit::{AppContext as _, ElementId, TestAppContext};
use protocol::model::{WorkspaceId, WorkspaceStatus};

use crate::support::{
    FakeHub, Held, MORE_THAN_FIT, Stack, Ui, after_every_workspace, open_on_the_world, order, sidebar_row,
    three_workspaces, top, with_a_chat, worked_in,
};

/// A workspace the hub is making, other than those in `known`, once the app has heard of it and the last frame drew its
/// note in the sidebar, and the note.
fn on_its_way(ui: &mut Ui, known: &[WorkspaceId]) -> Option<(WorkspaceId, ElementId)> {
    let id = ui
        .view()
        .records()
        .workspaces()
        .iter()
        .find(|w| w.status == WorkspaceStatus::Creating && !known.contains(&w.id))
        .map(|w| w.id.clone())?;
    let note = ElementId::Name(format!("creating-{id}").into());
    ui.has(note.clone()).then_some((id, note))
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

/// The window on a fake hub holding `count` workspaces in one repository, each with a chat, made one after another,
/// and nobody has worked in any since, so the newest is at the top. The workspaces are oldest first.
fn workspaces_on_a_fake_hub(cx: &mut TestAppContext, count: usize) -> (FakeHub, Ui<'_>, Vec<WorkspaceId>) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let made = Vec::from_fn(count, |i| with_a_chat(&mut world, &repo, &format!("ws-{i}")));
    let (fake, mut ui) = open_on_the_world(cx, world);
    ui.wait("every workspace", |ui| ui.view().visible_workspaces().len() == count);
    (fake, ui, made)
}

#[gpui_kit::test]
fn the_sidebar_starts_last_active_first_and_the_heading_chooses_another_and_the_keys_go_in_its_order(
    cx: &mut TestAppContext,
) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let [first, second, third] = ["first", "second", "third"].map(|name| with_a_chat(&mut world, &repo, name));
    // The person has worked in the first since the others were made, as the hub notes on its record.
    world.workspace_mut(&first).user_active_at_ms = Some(NOW_MS);
    let (_fake, mut ui) = open_on_the_world(cx, world);
    ui.wait("every workspace", |ui| ui.view().visible_workspaces().len() == 3);
    assert!(!ui.visible("sort-workspaces"), "the button shows only while the pointer is on the heading");
    let made = [first.clone(), second.clone(), third.clone()];

    let sorted = [first.clone(), third.clone(), second.clone()];
    assert_eq!(
        order(&mut ui, sidebar_row, &made),
        sorted,
        "the one worked in, then the newest, which nobody has worked in"
    );
    ui.click(sidebar_row(&first));
    ui.press("secondary-]");
    assert_eq!(ui.view().selected.as_ref(), Some(&third), "the next one down the sidebar");

    sort_by(&mut ui, "Oldest first");

    assert_eq!(order(&mut ui, sidebar_row, &made), made, "the oldest first");
    ui.press("secondary-[");
    assert_eq!(ui.view().selected.as_ref(), Some(&second), "the one above it now");
}

/// As the hub hears a workspace was worked in, which sends it to the top.
#[gpui_kit::test]
fn a_row_slides_to_its_new_place_and_comes_to_rest_there(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [_, chat, _]) = three_workspaces(cx, &stack);
    let (was, goes) = (top(&mut ui, sidebar_row(&second)), top(&mut ui, sidebar_row(&third)));
    assert!(goes < was, "the newest is at the top: {goes:?}, {was:?}");
    ui.allow_motion();

    worked_in(&mut ui, &stack, &chat);

    assert_eq!(top(&mut ui, sidebar_row(&second)), was, "it starts from where it was, since no time has passed");
    ui.play(look::motion::PANEL / 2);
    let halfway = top(&mut ui, sidebar_row(&second));
    assert!(goes < halfway && halfway < was, "{halfway:?} isn't between {goes:?} and {was:?}");
    ui.play(look::motion::PANEL);
    assert_eq!(top(&mut ui, sidebar_row(&second)), goes);
    assert_eq!(ui.play(look::motion::PANEL), 0, "and nothing asks for another frame");
    assert_eq!(order(&mut ui, sidebar_row, &[first.clone(), second.clone(), third.clone()]), [second, third, first]);
}

/// The list builds the rows drawn in sight in each frame, wherever their places are. Oldest first sends the newest,
/// at the top, down below the window, and brings the oldest up from below it. A row going out of sight is still drawn
/// on its way there, rather than going at once. A row coming up from below the window isn't built while it's drawn
/// below the window, though its place is in sight, and is once it's drawn in sight, partway up.
#[gpui_kit::test]
fn a_row_going_out_of_sight_is_drawn_on_its_way_and_one_coming_into_sight_is_built_once_its_drawn_there(
    cx: &mut TestAppContext,
) {
    let (_fake, mut ui, made) = workspaces_on_a_fake_hub(cx, MORE_THAN_FIT);
    let (oldest, newest) = (&made[0], &made[MORE_THAN_FIT - 1]);
    assert!(!ui.has(sidebar_row(oldest)), "the oldest is below the window");
    let first = top(&mut ui, sidebar_row(newest));
    ui.allow_motion();

    sort_by(&mut ui, "Oldest first");

    assert_eq!(
        top(&mut ui, sidebar_row(newest)),
        first,
        "going down: it starts from where it was, as no time has passed"
    );
    assert!(
        !ui.has(sidebar_row(oldest)),
        "coming up: it's still drawn below the window, where it was, so it isn't built"
    );
    ui.play(look::motion::PANEL / 64);
    let going = top(&mut ui, sidebar_row(newest));
    assert!(going > first, "going down: it's drawn on its way down, at {going:?} from {first:?}");
    for frame in 1.. {
        if ui.has(sidebar_row(oldest)) {
            break;
        }
        assert!(frame < 64, "coming up: it never came into sight");
        ui.play(look::motion::PANEL / 64);
    }
    let coming = top(&mut ui, sidebar_row(oldest));
    assert!(coming > first, "coming up: it's built partway up, at {coming:?}, on its way to {first:?}");
    ui.settle(look::motion::PANEL / 4);
    assert!(!ui.has(sidebar_row(newest)), "going down: and once it's there, it's out of sight");
    assert_eq!(top(&mut ui, sidebar_row(oldest)), first, "coming up: and it comes to rest at the top");
}

/// The order holds still while someone could be about to click: the pointer on the list, and a row's menu open, since
/// its row moving would close it. Each case works in another workspace, which would send it to the top.
#[gpui_kit::test]
fn the_order_holds_still_while_the_pointer_is_on_the_list_or_a_rows_menu_is_open_and_changes_once_neither_is(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [first_chat, second_chat, third_chat]) = three_workspaces(cx, &stack);
    let made = [first.clone(), second.clone(), third.clone()];
    ui.hover(sidebar_row(&first));

    worked_in(&mut ui, &stack, &second_chat);

    let shown = order(&mut ui, sidebar_row, &made);
    assert_eq!(
        shown,
        [third.clone(), second.clone(), first.clone()],
        "the pointer on it: so it can't move from a click"
    );
    ui.press("secondary-]");
    assert_eq!(ui.view().selected.as_ref(), Some(&second), "the pointer on it: the keys go in the order shown");
    ui.hover("send");
    let shown = order(&mut ui, sidebar_row, &made);
    assert_eq!(shown, [second.clone(), third.clone(), first.clone()], "the pointer on it: until it leaves");

    ui.right_click(sidebar_row(&third));
    ui.hover("send");
    worked_in(&mut ui, &stack, &first_chat);

    let shown = order(&mut ui, sidebar_row, &made);
    assert_eq!(shown, [second.clone(), third.clone(), first.clone()], "a row's menu open: or the menu would go");
    assert!(menu_shows(&mut ui, "Archive"), "a row's menu open: the menu is still there");
    ui.press("escape");
    let shown = order(&mut ui, sidebar_row, &made);
    assert_eq!(shown, [first.clone(), second.clone(), third.clone()], "a row's menu open: until it closes");

    ui.hover(sidebar_row(&third));
    worked_in(&mut ui, &stack, &third_chat);
    // With nothing typed into, so no cursor blinks and draws the window again meanwhile, and the order shows only if
    // the end of the hold draws it.
    ui.cx.update_window(ui.handle, |_, window, cx| window.blur(cx)).unwrap();
    ui.frame();

    let a_moment = Duration::from_millis(100);
    ui.cx.executor().advance_clock(desktop::ORDER_HOLDS_FOR.checked_sub(a_moment).unwrap());
    ui.frame();
    let shown = order(&mut ui, sidebar_row, &made);
    assert_eq!(shown, [first.clone(), second.clone(), third.clone()], "the pointer resting on it: not quite yet");
    ui.cx.executor().advance_clock(a_moment);
    ui.frame();
    let shown = order(&mut ui, sidebar_row, &made);
    assert_eq!(shown, [third, first, second], "the pointer resting on it: for a moment, though it's still there");
}

/// The order doesn't hold for a pointer the list can't have heard of since: one on it before Settings hid it, and one
/// left there once a key is pressed, until it moves again, so the order goes on changing while someone types.
#[gpui_kit::test]
fn the_order_doesnt_hold_for_a_pointer_from_before_settings_hid_the_list_or_one_resting_while_keys_are_pressed(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [first_chat, second_chat, _]) = three_workspaces(cx, &stack);
    let made = [first.clone(), second.clone(), third.clone()];
    ui.hover(sidebar_row(&first));
    // From the menu bar, since a key would end the hold by itself.
    ui.cx.update(|cx| cx.dispatch_action(&desktop::OpenSettings));
    ui.frame();

    worked_in(&mut ui, &stack, &second_chat);
    ui.press("escape");

    let shown = order(&mut ui, sidebar_row, &made);
    assert_eq!(
        shown,
        [second.clone(), third.clone(), first.clone()],
        "after Settings: it's heard nothing of the pointer"
    );

    ui.hover("send");
    ui.hover(sidebar_row(&first));
    ui.press("a");
    worked_in(&mut ui, &stack, &first_chat);

    assert_eq!(order(&mut ui, sidebar_row, &made), [first, second, third], "after a key: so it goes on changing");
}

/// Asked for by another client, such as the command line, so only the hub can say it's on its way, and then made with
/// `Cmd-N`, so the pointer isn't on the list when Create is pressed. Either way the note starts where the workspace
/// will go, and the workspace takes its place.
#[gpui_kit::test]
fn a_new_workspaces_note_shows_where_it_will_go_and_keeps_its_place_for_it_once_the_order_holds(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], _) = three_workspaces(cx, &stack);
    let making = Held::checkout(&stack);
    let (client, repo) = (stack.client(), ui.first_repo_id());
    let known = [first.clone(), second.clone(), third.clone()];
    after_every_workspace(&ui);

    let asking = std::thread::spawn(move || client.workspace_create(&repo, None, None, None).unwrap().id);

    ui.wait("another client's note", |ui| on_its_way(ui, &known).is_some());
    let (coming, note) = on_its_way(&mut ui, &known).unwrap();
    assert!(top(&mut ui, note) < top(&mut ui, sidebar_row(&third)), "another client's: at the top, where it will go");
    ui.press("secondary-[");
    assert_eq!(ui.view().selected.as_ref(), Some(&first), "another client's: the keys go past it, round to the bottom");
    making.release();
    let theirs = asking.join().unwrap();
    assert_eq!(theirs, coming, "another client's: the workspace the note was for");
    ui.wait("another client's row", |ui| ui.has(sidebar_row(&theirs)));
    let shown = order(&mut ui, sidebar_row, &[first.clone(), second.clone(), third.clone(), theirs.clone()]);
    assert_eq!(shown, [theirs.clone(), third.clone(), second.clone(), first.clone()], "another client's: at the top");
    assert_eq!(ui.view().selected.as_ref(), Some(&first), "another client's: and it doesn't take the window over");

    let known = [first.clone(), second.clone(), third.clone(), theirs.clone()];
    making.hold_again();
    ui.press("secondary-n");
    ui.wait("the new-workspace dialog", Ui::has_dialog);
    after_every_workspace(&ui);
    ui.press("enter");
    ui.wait("Cmd-N's note", |ui| on_its_way(ui, &known).is_some());
    let (coming, note) = on_its_way(&mut ui, &known).unwrap();
    let at = top(&mut ui, note.clone());
    assert!(at < top(&mut ui, sidebar_row(&theirs)), "Cmd-N's: it's at the top, where the new one will go");

    ui.hover(sidebar_row(&first));

    assert_eq!(top(&mut ui, note), at, "Cmd-N's: the pointer coming onto the list moves nothing");
    making.release();
    ui.wait("Cmd-N's row", |ui| ui.has(sidebar_row(&coming)));
    let made = ui.wait_for_a_new_workspace(known.len());
    assert_eq!(made, coming, "Cmd-N's: the workspace the note was for");
    assert_eq!(top(&mut ui, sidebar_row(&made)), at, "Cmd-N's: takes its place");
    let shown =
        order(&mut ui, sidebar_row, &[first.clone(), second.clone(), third.clone(), theirs.clone(), made.clone()]);
    assert_eq!(shown, [made, theirs, third, second, first], "Cmd-N's: at the top, while the order holds");
}
