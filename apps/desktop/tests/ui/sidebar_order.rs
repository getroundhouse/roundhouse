//! The order of the sidebar's workspaces, which its heading chooses, and the rows sliding as it changes.

#![cfg(test)]

use std::time::Duration;

use gpui_kit::test::TestQueryExt as _;
use gpui_kit::{AppContext as _, ElementId, TestAppContext};
use protocol::model::{AgentKind, SessionSettingsChange, WorkspaceId, WorkspaceStatus};

use crate::support::{
    Held, MORE_THAN_FIT, Stack, Ui, more_than_fit, order, sidebar_row, three_workspaces, top, worked_in,
};

/// The workspace the hub is making, once the app has heard of it and the last frame drew its note in the sidebar, and
/// the note.
fn on_its_way(ui: &mut Ui) -> Option<(WorkspaceId, ElementId)> {
    let id = ui
        .view()
        .records()
        .workspaces()
        .iter()
        .find(|w| w.status == WorkspaceStatus::Creating)
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
    assert_eq!(
        order(&mut ui, sidebar_row, &made),
        sorted,
        "the one worked in, then the newest, which nobody has worked in"
    );
    ui.click(sidebar_row(&first));
    ui.press("secondary-]");
    assert_eq!(ui.view().selected.as_ref(), Some(&third), "the next one down the sidebar");

    sort_by(&mut ui, "Oldest first");

    assert_eq!(order(&mut ui, sidebar_row, &made), made);
    ui.press("secondary-[");
    assert_eq!(ui.view().selected.as_ref(), Some(&second), "the one above it now");
}

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

/// The list builds the rows drawn in sight in each frame, wherever their places are. A row whose place is now below
/// the window is still drawn on its way there, rather than going at once.
#[gpui_kit::test]
fn a_row_going_down_out_of_sight_is_drawn_on_its_way_there(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, made) = more_than_fit(cx, &stack);
    // Nobody has worked in any, so the newest is first, and oldest first puts it below the window.
    let newest = &made[MORE_THAN_FIT - 1];
    let was = top(&mut ui, sidebar_row(newest));
    ui.allow_motion();

    sort_by(&mut ui, "Oldest first");

    assert_eq!(top(&mut ui, sidebar_row(newest)), was, "it starts from where it was, since no time has passed");
    ui.play(look::motion::PANEL / 16);
    let on_its_way = top(&mut ui, sidebar_row(newest));
    assert!(on_its_way > was, "it's drawn on its way down, at {on_its_way:?} from {was:?}");
    ui.settle(look::motion::PANEL / 4);
    assert!(!ui.has(sidebar_row(newest)), "and once it's there, it's out of sight");
}

/// A row on its way up from below the window isn't built while it's drawn below the window, though its place is in
/// sight, and is once it's drawn in sight, partway up.
#[gpui_kit::test]
fn a_row_coming_up_from_below_the_window_is_built_once_its_drawn_in_sight(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let client = stack.client();
    let (mut ui, made) = more_than_fit(cx, &stack);
    // Nobody has worked in any, so the newest is first and the oldest is below the window.
    let (oldest, newest) = (&made[0], &made[MORE_THAN_FIT - 1]);
    assert!(!ui.has(sidebar_row(oldest)));
    let first = top(&mut ui, sidebar_row(newest));
    ui.allow_motion();

    // Making a chat in it counts as working in it, which sends it to the top once the app has heard that from its
    // workspace's record, which comes apart from the chat's.
    let worked_in = |ui: &Ui| ui.view().records().workspace(oldest).and_then(|w| w.user_active_at_ms);
    let before = worked_in(&ui);
    client.session_create(oldest, AgentKind::Claude, SessionSettingsChange::default(), None).unwrap();
    ui.wait("it to be worked in", |ui| worked_in(ui) != before);

    assert!(!ui.has(sidebar_row(oldest)), "it's still drawn below the window, where it was, so it isn't built");
    for frame in 0.. {
        assert!(frame < 64, "it never came into sight");
        ui.play(look::motion::PANEL / 64);
        if ui.has(sidebar_row(oldest)) {
            break;
        }
    }
    let coming = top(&mut ui, sidebar_row(oldest));
    assert!(coming > first, "it's built partway up, at {coming:?}, on its way to {first:?}");
    ui.settle(look::motion::PANEL / 4);
    assert_eq!(top(&mut ui, sidebar_row(oldest)), first, "and it comes to rest at the top");
}

#[gpui_kit::test]
fn the_order_holds_still_while_the_pointer_is_on_the_list_and_changes_once_it_leaves(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [_, chat, _]) = three_workspaces(cx, &stack);
    let made = [first.clone(), second.clone(), third.clone()];
    ui.hover(sidebar_row(&first));

    worked_in(&mut ui, &stack, &chat);

    assert_eq!(
        order(&mut ui, sidebar_row, &made),
        [third.clone(), second.clone(), first.clone()],
        "so it can't move from a click"
    );
    ui.press("secondary-]");
    assert_eq!(ui.view().selected.as_ref(), Some(&second), "the keys go in the order shown");
    ui.hover("send");
    assert_eq!(order(&mut ui, sidebar_row, &made), [second, third, first]);
}

#[gpui_kit::test]
fn the_order_changes_once_the_pointer_has_rested_on_the_list_for_a_moment(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [_, chat, _]) = three_workspaces(cx, &stack);
    let made = [first.clone(), second.clone(), third.clone()];
    ui.hover(sidebar_row(&first));

    worked_in(&mut ui, &stack, &chat);
    // With nothing typed into, so no cursor blinks and draws the window again meanwhile, and the order shows only if
    // the end of the hold draws it.
    ui.cx.update_window(ui.handle, |_, window, cx| window.blur(cx)).unwrap();
    ui.frame();

    let a_moment = Duration::from_millis(100);
    ui.cx.executor().advance_clock(desktop::ORDER_HOLDS_FOR.checked_sub(a_moment).unwrap());
    ui.frame();
    assert_eq!(order(&mut ui, sidebar_row, &made), [third.clone(), second.clone(), first.clone()], "not quite yet");
    ui.cx.executor().advance_clock(a_moment);
    ui.frame();
    assert_eq!(order(&mut ui, sidebar_row, &made), [second, third, first], "with the pointer still there");
}

#[gpui_kit::test]
fn the_order_doesnt_hold_for_a_pointer_that_was_on_the_list_before_settings_hid_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [_, chat, _]) = three_workspaces(cx, &stack);
    let made = [first.clone(), second.clone(), third.clone()];
    ui.hover(sidebar_row(&first));
    // From the menu bar, since a key would end the hold by itself, as the next test says.
    ui.cx.update(|cx| cx.dispatch_action(&desktop::OpenSettings));
    ui.frame();

    worked_in(&mut ui, &stack, &chat);
    ui.press("escape");

    assert_eq!(
        order(&mut ui, sidebar_row, &made),
        [second, third, first],
        "since it's heard nothing of the pointer since"
    );
}

#[gpui_kit::test]
fn the_order_doesnt_hold_once_a_key_is_pressed_until_the_pointer_moves_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [_, chat, _]) = three_workspaces(cx, &stack);
    let made = [first.clone(), second.clone(), third.clone()];
    ui.hover(sidebar_row(&first));
    ui.press("a");

    worked_in(&mut ui, &stack, &chat);

    assert_eq!(
        order(&mut ui, sidebar_row, &made),
        [second, third, first],
        "so it goes on changing while someone types"
    );
}

#[gpui_kit::test]
fn the_order_holds_still_while_a_rows_menu_is_open_wherever_the_pointer_goes(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], [_, chat, _]) = three_workspaces(cx, &stack);
    let made = [first.clone(), second.clone(), third.clone()];
    ui.right_click(sidebar_row(&second));
    ui.hover("send");

    worked_in(&mut ui, &stack, &chat);

    assert_eq!(
        order(&mut ui, sidebar_row, &made),
        [third.clone(), second.clone(), first.clone()],
        "or the menu would go"
    );
    assert!(menu_shows(&mut ui, "Archive"), "the menu is still there");
    ui.press("escape");
    assert_eq!(order(&mut ui, sidebar_row, &made), [second, third, first]);
}

/// Made with `Cmd-N`, so the pointer isn't on the list when Create is pressed, and the note starts where the workspace
/// will go.
#[gpui_kit::test]
fn a_new_workspaces_note_keeps_its_place_once_the_order_holds_and_the_workspace_takes_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], _) = three_workspaces(cx, &stack);
    let making = Held::checkout(&stack);
    ui.press("secondary-n");
    ui.wait("the new-workspace dialog", Ui::has_dialog);
    ui.press("enter");
    ui.wait("the note", |ui| on_its_way(ui).is_some());
    let (coming, note) = on_its_way(&mut ui).unwrap();
    let at = top(&mut ui, note.clone());
    assert!(at < top(&mut ui, sidebar_row(&third)), "it's at the top, where the new one will go");

    ui.hover(sidebar_row(&first));

    assert_eq!(top(&mut ui, note), at, "the pointer coming onto the list moves nothing");
    making.release();
    let made = ui.wait_for_a_new_workspace(3);
    assert_eq!(made, coming, "the workspace the note was for");
    assert_eq!(top(&mut ui, sidebar_row(&made)), at, "takes its place");
    let shown = order(&mut ui, sidebar_row, &[first.clone(), second.clone(), third.clone(), made.clone()]);
    assert_eq!(shown, [made, third, second, first]);
}

/// Asked for by another client, such as the command line, so only the hub can say it's on its way.
#[gpui_kit::test]
fn a_workspace_another_client_is_making_shows_on_its_way_where_it_will_go(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second, third], _) = three_workspaces(cx, &stack);
    let making = Held::checkout(&stack);
    let (client, repo) = (stack.client(), ui.first_repo_id());

    let asking = std::thread::spawn(move || client.workspace_create(&repo, None, None, None).unwrap().id);

    ui.wait("the note", |ui| on_its_way(ui).is_some());
    let (coming, note) = on_its_way(&mut ui).unwrap();
    assert!(top(&mut ui, note) < top(&mut ui, sidebar_row(&third)), "at the top, where it will go");
    ui.press("secondary-[");
    assert_eq!(ui.view().selected.as_ref(), Some(&first), "the keys go past it, round to the bottom");
    making.release();
    let made = asking.join().unwrap();
    assert_eq!(made, coming);
    ui.wait("its row", |ui| ui.has(sidebar_row(&made)));
    let shown = order(&mut ui, sidebar_row, &[first.clone(), second.clone(), third.clone(), made.clone()]);
    assert_eq!(shown, [made, third, second, first.clone()]);
    assert_eq!(ui.view().selected.as_ref(), Some(&first), "and it doesn't take the window over");
}
