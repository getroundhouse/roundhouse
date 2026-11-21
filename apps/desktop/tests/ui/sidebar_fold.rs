//! A project's row in the sidebar: collapsing the project, which puts its workspaces away under the row, and bringing
//! them back. `sidebar_drag` has dragging it to another place among the projects. Which projects are collapsed is the
//! app's own, kept in its settings, so a test of only that opens on a fake hub, and one of what follows a hub's change,
//! such as a workspace made or archived, on a real one.

#![cfg(test)]

use client_core::fake_hub::World;
use client_core::fake_hub::events::{finished_turn, first_turn, text, user_prompt};
use gpui_kit::{Pixels, SharedString, TestAppContext, px};
use protocol::model::{RepoId, SessionStatus, WorkspaceId};

use crate::support::{
    FakeHub, MORE_THAN_FIT, Stack, Ui, home_row, open_on_the_world, order, project_row, sidebar_row as row, top,
    two_projects, two_projects_world, with_a_chat,
};

/// Where the next project's row starts once `project` is collapsed: under its row, as it's drawn, and the 8px between
/// projects.
fn after_collapsed(ui: &mut Ui, project: &RepoId) -> Pixels {
    ui.bounds(project_row(project)).bottom() + px(8.)
}

/// The window on `world`, once it shows every one of its workspaces.
fn open_on(cx: &mut TestAppContext, world: World) -> (FakeHub, Ui<'_>) {
    let listed = world.workspaces.len();
    let (fake, mut ui) = open_on_the_world(cx, world);
    ui.wait("every workspace", |ui| ui.view().visible_workspaces().len() == listed);
    (fake, ui)
}

#[gpui_kit::test]
fn pressing_a_projects_row_collapses_it_and_pressing_it_again_brings_its_workspaces_back(cx: &mut TestAppContext) {
    let (world, [first, second], [upper, lower, third, _]) = two_projects_world();
    let (fake, mut ui) = open_on(cx, world);
    let was = top(&mut ui, project_row(&second));
    let showing = ui.view().selected().cloned();

    ui.click(project_row(&first));

    assert!(!ui.has(row(&upper)) && !ui.has(row(&lower)), "its workspaces are put away");
    assert!(ui.has(row(&third)), "and the other project's aren't");
    let closed = top(&mut ui, project_row(&second));
    assert_eq!(closed, after_collapsed(&mut ui, &first), "the next project comes straight after it");
    assert_eq!(ui.view().selected(), showing.as_ref(), "what's showing is still showing");
    let saved = || std::fs::read_to_string(fake.settings_file()).unwrap_or_default();
    ui.wait("it to be saved", |_| saved().contains(first.as_str()));

    ui.click(project_row(&first));

    assert!(ui.has(row(&upper)) && ui.has(row(&lower)), "they're back");
    assert_eq!(top(&mut ui, project_row(&second)), was);
    ui.wait("that to be saved", |_| !saved().contains(first.as_str()));
}

/// A project with no workspaces has nothing to collapse, and a collapsed one brings its workspaces back for a new
/// one, so either way the workspace made shows. The button for a new workspace is on the project's row, and pressing it
/// doesn't collapse the project.
#[gpui_kit::test]
fn a_new_workspace_shows_in_a_project_whose_row_was_pressed_with_nothing_to_collapse_or_collapsed(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let repo = ui.first_repo_id();

    ui.click(project_row(&repo));

    let made = stack.client().workspace_create(&repo, None, None, None).unwrap().id;
    ui.wait("with nothing to collapse: the workspace", |ui| ui.view().visible_workspaces().len() == 1);
    assert!(
        ui.has(row(&made)),
        "with nothing to collapse: its first workspace shows, since pressing its row did nothing"
    );

    ui.open_new_workspace();

    assert!(ui.has(row(&made)), "the button for a new workspace is on the project's row, and isn't the row");
    ui.press("escape");
    ui.wait_out_the_dialog_going();
    ui.click(project_row(&repo));
    assert!(!ui.has(row(&made)), "collapsed: its workspace is put away");

    let newer = ui.new_workspace();

    assert!(ui.has(row(&newer)), "collapsed: the workspace it opened has its row");
    assert!(ui.has(row(&made)), "collapsed: with the project's other");
}

/// With both projects collapsed, only the one with a reply nobody has read says so.
#[gpui_kit::test]
fn a_collapsed_project_says_how_many_of_its_workspaces_need_a_look(cx: &mut TestAppContext) {
    let (mut world, [first, second], [.., last]) = two_projects_world();
    let reply = first_turn(user_prompt("Look at the ledger."), [text("msg-1", "Done."), finished_turn(4, 0.01)]);
    let chat = world.chat(&last, SessionStatus::Idle, reply);
    world.unread(&chat);
    let (_fake, mut ui) = open_on(cx, world);
    let count = |repo: &RepoId| format!("project-needs-a-look-{repo}");
    assert_ne!(ui.view().selected(), Some(&last), "the workspace with the reply isn't the one showing");

    ui.click(project_row(&first));
    ui.click(project_row(&second));

    assert!(!ui.has(SharedString::from(count(&first))), "nothing in the first needs a look");
    assert!(ui.shows_label(&count(&second), "1 workspace needs a look"), "and one in the second does");
    ui.click(project_row(&second));
    assert!(ui.has(row(&last)), "its workspaces are back");
    assert!(!ui.has(SharedString::from(count(&second))), "and their rows say so themselves");
}

#[gpui_kit::test]
fn opening_a_workspace_from_home_brings_back_its_projects_workspaces(cx: &mut TestAppContext) {
    let (world, [_, second], [.., third, last]) = two_projects_world();
    let (_fake, mut ui) = open_on(cx, world);
    ui.click(project_row(&second));
    assert!(!ui.has(row(&last)));

    ui.click("home");
    ui.click(home_row(&last));

    assert_eq!(ui.view().selected(), Some(&last));
    assert!(ui.has(row(&last)) && ui.has(row(&third)), "so the workspace showing has its row");
}

/// The one in its place is the top of what the sidebar shows, not of a collapsed project above it, and with every
/// project collapsed, the top of what it lists.
#[gpui_kit::test]
fn once_the_workspace_showing_has_gone_the_one_in_its_place_is_the_top_of_what_shows_or_else_of_what_is_listed(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let (mut ui, [first, second], [_, _, third, last]) = two_projects(cx, &stack);
    ui.click(row(&third));
    ui.click(project_row(&first));

    stack.client().workspace_archive(&third, false).unwrap();

    ui.wait("one project collapsed: the one archived to go", |ui| ui.view().selected() != Some(&third));
    assert_eq!(ui.view().selected(), Some(&last), "one project collapsed: the top of what shows");

    ui.click(project_row(&second));
    stack.client().workspace_archive(&last, false).unwrap();

    ui.wait("every project collapsed: the one archived to go", |ui| ui.view().selected() != Some(&last));
    assert!(
        ui.view().selected().is_some(),
        "every project collapsed: the top of what it lists, since the sidebar shows none"
    );
}

#[gpui_kit::test]
fn the_keys_pass_over_a_collapsed_projects_workspaces_and_go_on_from_where_the_one_showing_would_be(
    cx: &mut TestAppContext,
) {
    let (world, [first, second], made) = two_projects_world();
    let (_fake, mut ui) = open_on(cx, world);
    let [upper, lower, third, last]: [WorkspaceId; 4] = order(&mut ui, row, &made).try_into().unwrap();
    ui.click(row(&lower));
    ui.click(project_row(&first));
    assert_eq!(ui.view().selected(), Some(&lower), "it's still showing, with its project collapsed");

    ui.press("alt-secondary-down");

    assert_eq!(ui.view().selected(), Some(&third), "the first one showing below where its row would be");
    assert!(!ui.has(row(&lower)), "which doesn't bring the project it left back");
    ui.press("alt-secondary-up");
    assert_eq!(ui.view().selected(), Some(&last), "around to the last, over the collapsed project's two");

    ui.click(project_row(&first));
    ui.click(project_row(&second));
    assert_eq!(ui.view().selected(), Some(&last));
    ui.press("alt-secondary-up");
    assert_eq!(ui.view().selected(), Some(&lower), "the last one showing above where its row would be");
    ui.press("alt-secondary-down");
    assert_eq!(ui.view().selected(), Some(&upper), "around to the first, over the collapsed project's two");
}

/// A fold turned back partway goes on from where it had got to, and one left to run comes to rest. Each starts from the
/// project open, at rest.
#[gpui_kit::test]
fn a_projects_workspaces_fold_away_under_what_slides_up_over_them_and_a_fold_turned_back_goes_on_from_where_it_got_to(
    cx: &mut TestAppContext,
) {
    let (world, [first, second], [upper, lower, ..]) = two_projects_world();
    let (_fake, mut ui) = open_on(cx, world);
    let (was, rows) = (top(&mut ui, project_row(&second)), top(&mut ui, row(&upper)));
    let goes = after_collapsed(&mut ui, &first);
    ui.allow_motion();
    ui.click(project_row(&first));
    ui.play(look::motion::PANEL / 8);
    let turned_at = top(&mut ui, project_row(&second));
    assert!(turned_at < was, "turned back: it had started up");

    ui.click(project_row(&first));

    assert_eq!(top(&mut ui, project_row(&second)), turned_at, "turned back: it doesn't jump");
    ui.play(look::motion::PANEL / 8);
    let back = top(&mut ui, project_row(&second));
    assert!(turned_at < back && back < was, "turned back: {back:?} isn't between {turned_at:?} and {was:?}");
    ui.settle(look::motion::PANEL / 4);
    assert_eq!(top(&mut ui, project_row(&second)), was, "turned back: it comes to rest where it was");
    assert!(ui.has(row(&upper)) && ui.has(row(&lower)), "turned back: with its workspaces showing");

    ui.click(project_row(&first));

    assert_eq!(
        top(&mut ui, project_row(&second)),
        was,
        "left to run: it starts from where it was, as no time has passed"
    );
    assert!(ui.has(row(&upper)) && ui.has(row(&lower)), "left to run: so both rows still show");
    ui.play(look::motion::PANEL / 2);
    let halfway = top(&mut ui, project_row(&second));
    assert!(goes < halfway && halfway < was, "left to run: {halfway:?} isn't between {goes:?} and {was:?}");
    assert_eq!(top(&mut ui, row(&upper)), rows, "left to run: the rows stay where they are");
    assert!(!ui.has(row(&lower)), "left to run: and the next project has covered the lower one");
    assert_eq!(ui.view().projects_folding(), 1, "left to run: halfway");
    ui.play(look::motion::PANEL);
    assert_eq!(top(&mut ui, project_row(&second)), goes, "left to run: it comes to rest under the collapsed project");
    assert!(!ui.has(row(&upper)), "left to run: with its workspaces put away");
    assert_eq!(ui.view().projects_folding(), 0, "left to run: the fold is over, so its rows are no longer lines");
    assert_eq!(ui.play(look::motion::PANEL), 0, "left to run: and nothing asks for another frame");
}

/// The list builds the lines drawn in sight, wherever their places are, so what comes after a long project, whose
/// place is far below the window as the project opens, is drawn on its way there.
#[gpui_kit::test]
fn what_comes_after_a_long_project_slides_out_of_sight_as_it_opens_rather_than_going_at_once(cx: &mut TestAppContext) {
    let (mut world, [first, second], _) = two_projects_world();
    let made = Vec::from_fn(MORE_THAN_FIT, |i| with_a_chat(&mut world, &first, &format!("more-{i}")));
    // The newest, which is the first project's first row.
    let upper = made.last().unwrap();
    let (_fake, mut ui) = open_on(cx, world);
    assert!(!ui.has(project_row(&second)), "the second project is below the window");
    ui.click(project_row(&first));
    let was = top(&mut ui, project_row(&second));
    ui.allow_motion();

    ui.click(project_row(&first));

    assert_eq!(top(&mut ui, project_row(&second)), was, "it's still where it was, since no time has passed");
    assert!(!ui.has(row(upper)), "over the first project's workspaces, of which none shows yet");
    ui.play(look::motion::PANEL / 16);
    let moved = top(&mut ui, project_row(&second));
    assert!(moved > was, "it's on its way down, {moved:?} from {was:?}");
    ui.settle(look::motion::PANEL / 4);
    assert!(!ui.has(project_row(&second)), "and it's out of sight once it's there");
    assert!(ui.has(row(upper)));
}
