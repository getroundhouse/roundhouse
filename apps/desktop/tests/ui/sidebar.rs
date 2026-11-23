//! The sidebar's list of workspaces. What a row shows of a workspace's record, and what the pointer and the keys do to
//! it, are the app's, so those open on a fake hub, with no worker or agent to wait for. Archiving, sending and adding a
//! project are the hub's, so those open on a real one.

#![cfg(test)]

use client_core::fake_hub::World;
use desktop::busy::Busy;
use gpui_kit::test::{TestEventExt as _, TestWindowExt as _};
use gpui_kit::{AppContext as _, ElementId, MouseButton, ScrollDelta, SharedString, TestAppContext, point, px};
use protocol::model::{LineCounts, Observation, Observed, Rev, WorkspaceId, WorkspaceStatus};

use crate::support::{
    FakeHub, Held, MORE_THAN_FIT, Stack, Ui, home_row, open_on_the_world, pencil, sidebar_row as row, with_a_chat,
};

/// Off every row, for the pointer to wait on.
const ELSEWHERE: &str = "workspace-title";

fn archive_button(id: &WorkspaceId) -> ElementId {
    ElementId::Name(SharedString::from(format!("archive-{id}")))
}

fn lines(id: &WorkspaceId) -> ElementId {
    ElementId::Name(SharedString::from(format!("lines-{id}")))
}

fn archived(ui: &Ui, id: &WorkspaceId) -> bool {
    let listed = ui.view().records().workspaces().iter().any(|w| &w.id == id && w.status == WorkspaceStatus::Archived);
    listed && ui.view().busy == Busy::Free
}

/// A window on a fake hub with two workspaces in one repository, each with a chat that's had nothing said in it, as a
/// new one's hasn't, and the second the newer. The hub goes with the test, so it's returned to be kept.
fn two_on_a_fake_hub(cx: &mut TestAppContext) -> (FakeHub, Ui<'_>, [WorkspaceId; 2]) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let made = ["ledger", "points"].map(|name| with_a_chat(&mut world, &repo, name));
    let (fake, ui) = open_on_the_world(cx, world);
    (fake, ui, made)
}

#[gpui_kit::test]
fn a_long_list_draws_only_the_workspaces_on_screen_and_scrolls_to_the_rest(cx: &mut TestAppContext) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let made = Vec::from_fn(MORE_THAN_FIT, |i| with_a_chat(&mut world, &repo, &format!("more-{i}")));
    let (_fake, mut ui) = open_on_the_world(cx, world);
    // Each was worked in as it was made, so the sidebar lists the newest first.
    let (first, last) = (row(&made[MORE_THAN_FIT - 1]), row(&made[0]));

    assert!(ui.has(first.clone()));
    assert!(!ui.has(last.clone()), "a row below the bottom isn't drawn");

    // Over a row, since the kit's test window only finds what it records, and the list itself isn't.
    ui.scroll(first.clone(), ScrollDelta::Pixels(point(px(0.), px(-10_000.))));

    assert!(ui.has(last), "scrolling reaches the last row");
    assert!(!ui.has(first), "a row above the top isn't drawn");
}

/// Once it's let go, archiving runs through, so the row's menu archives the workspace showing without waiting.
#[gpui_kit::test]
fn a_rows_archive_button_shows_under_the_pointer_and_archives_it_without_opening_it_as_its_menu_does(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let held = Held::archive(&stack);
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let (going, showing) = (ui.new_workspace(), ui.new_workspace());
    ui.hover("send");
    assert!(!ui.visible(archive_button(&going)), "the pointer is elsewhere");
    assert!(!ui.visible(archive_button(&showing)), "not even the workspace showing has one");
    ui.hover(row(&going));
    assert!(ui.visible(archive_button(&going)), "the pointer is over its row");
    assert!(!ui.visible(archive_button(&showing)), "only the row under the pointer has one");

    ui.click(archive_button(&going));

    ui.wait("it to start archiving", |_| held.runs() == 1);
    // Held open, so nothing that comes once it's archived has moved the selection yet.
    assert_eq!(ui.view().selected(), Some(&showing), "pressing it doesn't also open the workspace");
    ui.hover("send");
    // The row says so from the workspace's record, which reaches the app on the hub's feed, apart from the script the
    // hub runs once it has recorded it, so it's waited for rather than read as soon as the script has run.
    ui.wait("its row to say so, wherever the pointer is", |ui| ui.shows_label(&format!("doing-{going}"), "Archiving…"));
    assert!(!ui.has(SharedString::from(format!("doing-{showing}"))), "the workspace showing isn't going");
    held.release();
    ui.wait("it to be archived", |ui| archived(ui, &going));
    assert!(!ui.has(row(&going)), "it leaves the list");

    ui.right_click(row(&showing));
    ui.click_menu_item("Archive");

    ui.wait("the one showing to be archived by its menu", |ui| archived(ui, &showing));
    assert!(!ui.has(row(&showing)), "it leaves the list, and only Home shows it");
}

/// The hub's record says a workspace is being archived, so every client shows it, wherever it was asked for, such as
/// the command line, and this app goes on archiving others meanwhile. That a workspace leaves the list once its
/// record says it's archived is the test above's.
#[gpui_kit::test]
fn a_workspace_being_archived_from_elsewhere_says_so_here_and_another_can_still_be_archived(cx: &mut TestAppContext) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let [going, other] = ["ledger", "points"].map(|name| with_a_chat(&mut world, &repo, name));
    world.workspace_mut(&going).status = WorkspaceStatus::Archiving;

    let (_fake, mut ui) = open_on_the_world(cx, world);

    ui.wait("its row to say so", |ui| ui.shows_label(&format!("doing-{going}"), "Archiving…"));
    assert_eq!(ui.view().busy, Busy::Free, "this app asked for nothing");
    ui.hover(row(&other));
    assert!(ui.on(archive_button(&other)), "so it can archive another");
}

/// A quick press is made as the pointer comes to rest, so it moves while the button is down. Once pressed, the row says
/// the pointer has left it at the first move, which would take the button away from under the press.
#[gpui_kit::test]
fn a_rows_archive_button_stays_under_a_press_while_the_pointer_moves(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let held = Held::archive(&stack);
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let (going, showing) = (ui.new_workspace(), ui.new_workspace());
    ui.hover(row(&going));
    let button = archive_button(&going);

    let (drawn, at) = ui
        .cx
        .update_window(ui.handle, |_, window, cx| {
            let at = window.find(button.clone()).bounds().center();
            window.pointer_move(at, None, cx);
            window.pointer_down(at, MouseButton::Left, cx);
            let at = at + point(px(1.), px(0.));
            window.pointer_move(at, Some(MouseButton::Left), cx);
            (window.try_find(button.clone()).is_some(), at)
        })
        .unwrap();

    assert!(drawn, "the button is still there while it's held");

    ui.cx.update_window(ui.handle, |_, window, cx| window.pointer_up(at, MouseButton::Left, cx)).unwrap();

    ui.wait("it to start archiving", |_| held.runs() == 1);
    assert_eq!(ui.view().selected(), Some(&showing), "letting go doesn't open the workspace instead");
    held.release();
    ui.wait("it to be archived", |ui| archived(ui, &going));
}

/// A press on the row itself moves as it's made too, and the row then says the pointer has left, though it hasn't. The
/// button, and what made room for it, would go and come back around every click.
#[gpui_kit::test]
fn a_rows_archive_button_stays_while_the_row_is_pressed_and_goes_if_its_let_go_elsewhere(cx: &mut TestAppContext) {
    let (_fake, mut ui, [pressed, showing]) = two_on_a_fake_hub(cx);
    show(&mut ui, &showing);
    let button = archive_button(&pressed);
    ui.hover(row(&pressed));
    let on_row = ui.bounds(row(&pressed)).center();
    let held = Some(MouseButton::Left);
    let press_and_move = |ui: &mut Ui| {
        ui.cx
            .update_window(ui.handle, |_, window, cx| {
                window.pointer_down(on_row, MouseButton::Left, cx);
                window.pointer_move(on_row + point(px(1.), px(0.)), held, cx);
            })
            .unwrap();
    };

    press_and_move(&mut ui);
    assert!(ui.has(button.clone()), "the button is still there while the row is held");
    let elsewhere = ui.bounds(ELSEWHERE).center();
    ui.cx
        .update_window(ui.handle, |_, window, cx| {
            window.pointer_move(elsewhere, held, cx);
            window.pointer_up(elsewhere, MouseButton::Left, cx);
        })
        .unwrap();
    assert!(!ui.has(button.clone()), "let go off the row, the pointer has left it");
    assert_eq!(ui.view().selected(), Some(&showing), "and that's no click");

    ui.hover(row(&pressed));
    press_and_move(&mut ui);
    let at = on_row + point(px(1.), px(0.));
    ui.cx.update_window(ui.handle, |_, window, cx| window.pointer_up(at, MouseButton::Left, cx)).unwrap();
    ui.wait("the workspace", |ui| ui.view().selected() == Some(&pressed));
    assert!(ui.has(button.clone()), "let go on the row, the pointer is still over it");
    ui.hover(ELSEWHERE);
    assert!(!ui.has(button), "and it goes once the pointer does");
}

/// Shows the workspace, as its row does, and waits until it's showing.
fn show(ui: &mut Ui, id: &WorkspaceId) {
    ui.click(row(id));
    ui.wait("the workspace", |ui| ui.view().selected() == Some(id) && !ui.has("home-page"));
}

/// Shows Home in place of the workspace showing, and waits until it's there.
fn show_home(ui: &mut Ui) {
    ui.click("home");
    ui.wait("Home", |ui| ui.has("home-page"));
}

/// What's typed and not sent is kept in the app, so sending it, which only a real hub takes, is the next test's.
#[gpui_kit::test]
fn a_rows_pencil_shows_while_something_typed_in_it_isnt_sent(cx: &mut TestAppContext) {
    let (_fake, mut ui, [other, typed_in]) = two_on_a_fake_hub(cx);
    show(&mut ui, &typed_in);
    ui.input("  ");
    show(&mut ui, &other);
    assert!(!ui.has(pencil(&typed_in)), "spaces alone aren't a prompt");

    show(&mut ui, &typed_in);
    ui.input("half typed");
    assert!(!ui.has(pencil(&typed_in)), "not on the workspace showing, whose composer is in view");
    show_home(&mut ui);
    assert!(ui.visible(pencil(&typed_in)), "but while Home shows in its place");
    show(&mut ui, &other);
    assert!(ui.visible(pencil(&typed_in)), "and once another workspace shows");
    assert!(!ui.has(pencil(&other)), "nothing's typed in that one");
    ui.hover(row(&typed_in));
    assert!(ui.visible(pencil(&typed_in)) && ui.visible(archive_button(&typed_in)), "it shows beside the button");
}

#[gpui_kit::test]
fn a_rows_pencil_goes_once_whats_typed_in_it_is_sent(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let typed_in = ui.new_workspace();
    ui.input("half typed");
    show_home(&mut ui);
    assert!(ui.visible(pencil(&typed_in)), "while it isn't sent");

    show(&mut ui, &typed_in);
    ui.press("enter");

    ui.wait("the prompt to be taken", |ui| ui.composer_text().is_empty());
    // The composer empties as the prompt goes, and a prompt on its way is still unsent, so the pencil stays until the
    // hub has it.
    let (chat, client) = (ui.active_chat(), ui.view().client().unwrap());
    ui.wait("the hub to have the prompt", |_| !client.is_sending(&chat));
    show_home(&mut ui);
    assert!(!ui.has(pencil(&typed_in)), "sending it leaves nothing unsent");
}

/// The button's place is the row's right end, which a pencil rests in rather than beside, so no name is cut short for
/// a button that isn't there.
#[gpui_kit::test]
fn a_rows_pencil_rests_in_its_buttons_place_and_slides_aside_while_the_button_shows(cx: &mut TestAppContext) {
    let (_fake, mut ui, [other, typed_in]) = two_on_a_fake_hub(cx);
    show(&mut ui, &typed_in);
    ui.input("half typed");
    show(&mut ui, &other);
    // Somewhere with no tooltip, which would ask for frames of its own as it came.
    ui.hover(ELSEWHERE);
    ui.allow_motion();
    let repo = ui.view().records().repos()[0].id.clone();
    let under = ui.bounds(ElementId::Name(format!("new-in-{repo}").into())).center().x;
    let (pencil, button) = (pencil(&typed_in), archive_button(&typed_in));
    let rests = ui.bounds(pencil.clone());
    assert_eq!(rests.center().x, under, "under the project's button, where its own button goes");

    ui.hover(row(&typed_in));

    assert_eq!(ui.bounds(button.clone()).center().x, under, "the button is in its place at once, to be pressed");
    assert_eq!(ui.bounds(pencil.clone()), rests, "and the pencil sets off from where it was");
    ui.play(look::motion::FADE / 4);
    let aside = ui.bounds(button.clone()).left();
    let partway = ui.bounds(pencil.clone()).right();
    assert!(aside < partway && partway < rests.right(), "partway aside: {partway:?}");
    ui.settle(look::motion::FADE);
    assert_eq!(ui.bounds(pencil.clone()).right(), aside, "it ends against the button");

    ui.hover(ELSEWHERE);

    assert_eq!(ui.bounds(pencil.clone()).right(), aside, "it sets off back from there");
    ui.play(look::motion::FADE / 4);
    let partway = ui.bounds(pencil.clone()).right();
    assert!(aside < partway && partway < rests.right(), "partway back: {partway:?}");
    assert!(ui.has(button.clone()), "the button is still fading");
    ui.settle(look::motion::FADE);
    assert_eq!(ui.bounds(pencil.clone()), rests, "back where it rests");
    assert!(!ui.has(button), "and the button has gone");
    assert_eq!(ui.play(look::motion::FADE), 0, "nothing asks for another frame");
}

/// What the hub has read of each workspace's lines added and removed, whether it's current or as of when it was read,
/// and nothing for one it hasn't read or one with no changes, whose row has no side to draw.
#[gpui_kit::test]
fn a_rows_lines_show_once_the_hub_has_read_them_and_slide_aside_for_its_button(cx: &mut TestAppContext) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let [read, current, not_read, unchanged] =
        ["ledger", "points", "relay", "siding"].map(|name| with_a_chat(&mut world, &repo, name));
    let observed = |workspace_id: &WorkspaceId, rev, added, removed, as_of_ms: Option<i64>| {
        let value = LineCounts { added, removed };
        let lines = match as_of_ms {
            Some(as_of_ms) => Observed::AsOf { value, as_of_ms },
            None => Observed::Current { value },
        };
        Observation::WorkspaceLines { workspace_id: workspace_id.clone(), lines, rev: Rev::new(rev) }
    };
    world.observations = vec![
        observed(&read, 1, 37_412, 1_250, Some(1_000)),
        observed(&current, 2, 3, 0, None),
        observed(&unchanged, 3, 0, 0, None),
    ];
    let (_fake, mut ui) = open_on_the_world(cx, world);

    ui.wait("the lines read", |ui| ui.has(lines(&read)));

    assert!(ui.shows_label(&format!("lines-{read}"), "37412 lines added and 1250 removed, as last read"));
    assert!(ui.shows_label(&format!("lines-{current}"), "3 lines added"));
    assert!(!ui.has(lines(&not_read)), "nothing while they're not known");
    assert!(!ui.has(lines(&unchanged)), "nothing for no changes");
    let alone = ui.bounds(lines(&read));
    ui.hover(row(&read));
    ui.settle(look::motion::FADE);
    let button = ui.bounds(archive_button(&read));
    assert!(alone.right() > button.left() && ui.bounds(lines(&read)).right() <= button.left(), "aside for the button");
}

/// Create, at the sidebar's top, is off until there's a project to make a workspace in.
#[gpui_kit::test]
fn the_heading_adds_the_first_project_and_stays_to_add_another(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open(cx, &stack);
    assert!(ui.view().records().repos().is_empty());
    assert!(ui.off("create"), "with no project to make a workspace in");

    ui.click("add-project");
    let folder = stack.repo();
    ui.cx.simulate_path_prompt_response(move |_| Some(vec![folder]));

    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    assert!(ui.has("add-project"), "it's still there to add another");
    assert!(ui.on("create"), "with a project to make one in");
}

#[gpui_kit::test]
fn nothing_else_is_archived_or_restored_while_a_workspace_is_being_archived(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let github = stack.start_hub_reading_stand_in_github();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    // Made before the script, so archiving it isn't held.
    let earlier = ui.new_workspace();
    stack.client().workspace_archive(&earlier, false).unwrap();
    ui.wait("the earlier one to be archived", |ui| archived(ui, &earlier));
    let held = Held::archive(&stack);
    let other = ui.new_workspace();
    let id = ui.new_workspace();
    // Merged, so the pull request's bar offers Archive too.
    let (branch, at) = ui.push(&id);
    github.pull_requests.set_at(7, hub::GithubPullRequestState::Merged, &branch, &at);
    stack.client().workspace_pull_request(&id).unwrap();
    ui.wait("the pull request's Archive", |ui| ui.shows_label("pull-request-action", "Archive"));

    ui.press("secondary-shift-a");
    ui.wait("it to start archiving", |_| held.runs() == 1);
    assert!(ui.off("pull-request-action"), "the pull request's Archive can't be pressed meanwhile");
    // Why is the workspace's record's, which reaches the app on the hub's feed, apart from the script the hub runs once
    // it has recorded it, so it's waited for rather than read as soon as the script has run.
    ui.wait("the row to say why, in place of its button", |ui| ui.shows_label(&format!("doing-{id}"), "Archiving…"));
    assert!(ui.shows_label("pull-request-action", "Archiving…"), "and the pull request's Archive does too");
    ui.hover(row(&other));
    assert!(ui.off(archive_button(&other)), "another row's can't be pressed either");
    ui.press("secondary-shift-a");
    // Only Home lists an archived workspace, and pressing its row there brings it back.
    ui.click("home");
    ui.click("home-archived");
    ui.click_menu_item("Showing all");
    ui.click(home_row(&earlier));
    held.release();

    ui.wait_for_every_call();
    assert_eq!(held.runs(), 1, "the second press didn't archive it again");
    assert_eq!(ui.view().error, None);
    assert!(archived(&ui, &id));
    let workspaces = stack.client().workspace_list(true).unwrap();
    let restored = workspaces.iter().any(|w| w.id == earlier && w.status != WorkspaceStatus::Archived);
    assert!(!restored, "pressing an archived row meanwhile didn't restore it");
}
