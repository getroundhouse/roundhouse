//! A project's row in the sidebar: collapsing the project, which puts its workspaces away under the row, and bringing
//! them back. `sidebar_drag` has dragging it to another place among the projects.

#![cfg(test)]

use gpui_kit::{SharedString, TestAppContext, px};
use protocol::model::WorkspaceId;

use crate::support::{
    MORE_THAN_FIT, Stack, Ui, home_row, order, project_row, sidebar_row as row, top, two_projects, wait_out_setup,
};

#[gpui_kit::test]
fn pressing_a_projects_row_collapses_it_and_pressing_it_again_brings_its_workspaces_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second], [upper, lower, third, _]) = two_projects(cx, &stack);
    let was = top(&mut ui, project_row(&second));
    let showing = ui.view().selected.clone();

    ui.click(project_row(&first));

    assert!(!ui.has(row(&upper)) && !ui.has(row(&lower)), "its workspaces are put away");
    assert!(ui.has(row(&third)), "and the other project's aren't");
    let closed = top(&mut ui, project_row(&second));
    assert_eq!(closed, top(&mut ui, project_row(&first)) + px(40.), "the next project comes straight after it");
    assert_eq!(ui.view().selected, showing, "what's showing is still showing");
    let saved = |stack: &Stack| std::fs::read_to_string(stack.settings_file()).unwrap_or_default();
    ui.wait("it to be saved", |_| saved(&stack).contains(first.as_str()));

    ui.click(project_row(&first));

    assert!(ui.has(row(&upper)) && ui.has(row(&lower)), "they're back");
    assert_eq!(top(&mut ui, project_row(&second)), was);
    ui.wait("that to be saved", |_| !saved(&stack).contains(first.as_str()));
}

#[gpui_kit::test]
fn a_new_workspace_in_a_collapsed_project_brings_its_workspaces_back_and_its_button_doesnt_collapse_it(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let (mut ui, [first, _], [upper, lower, ..]) = two_projects(cx, &stack);

    ui.open_new_workspace();

    assert!(ui.has(row(&upper)), "the button for a new workspace is on the project's row, and isn't the row");
    ui.press("escape");
    ui.wait_out_the_dialog_going();
    ui.click(project_row(&first));
    assert!(!ui.has(row(&upper)));

    let made = ui.new_workspace();

    assert!(ui.has(row(&made)), "the workspace it opened has its row");
    assert!(ui.has(row(&upper)) && ui.has(row(&lower)), "with the project's others");
}

#[gpui_kit::test]
fn a_collapsed_project_says_how_many_of_its_workspaces_need_a_look(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.bring_to_front();
    let elsewhere = ui.new_workspace();
    let chat = ui.active_chat();
    ui.new_workspace();
    let repo = ui.first_repo_id();
    let count = format!("project-needs-a-look-{repo}");
    ui.click(project_row(&repo));
    assert!(!ui.has(SharedString::from(count.clone())), "nothing in it needs a look yet");

    stack.client().session_prompt(&chat, "say:while you were away", None).unwrap();

    ui.wait("it to say one does", |ui| ui.shows_label(&count, "1 workspace needs a look"));
    ui.click(project_row(&repo));
    assert!(ui.has(row(&elsewhere)), "its workspaces are back");
    assert!(!ui.has(SharedString::from(count)), "and their rows say so themselves");
}

#[gpui_kit::test]
fn a_project_with_no_workspaces_has_nothing_to_collapse(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let repo = ui.first_repo_id();

    ui.click(project_row(&repo));

    let made = stack.client().workspace_create(&repo, None, None, None).unwrap().id;
    ui.wait("the workspace", |ui| ui.view().visible_workspaces().len() == 1);
    assert!(ui.has(row(&made)), "so its first workspace shows, since pressing its row did nothing");
}

#[gpui_kit::test]
fn opening_a_workspace_from_home_brings_back_its_projects_workspaces(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [_, second], [.., third, last]) = two_projects(cx, &stack);
    ui.click(project_row(&second));
    assert!(!ui.has(row(&last)));

    ui.click("home");
    ui.click(home_row(&last));

    assert_eq!(ui.view().selected.as_ref(), Some(&last));
    assert!(ui.has(row(&last)) && ui.has(row(&third)), "so the workspace showing has its row");
}

#[gpui_kit::test]
fn with_every_project_collapsed_another_workspace_still_shows_once_the_one_showing_has_gone(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second], _) = two_projects(cx, &stack);
    ui.click(project_row(&first));
    ui.click(project_row(&second));
    let showing = ui.view().selected.clone().unwrap();

    stack.client().workspace_archive(&showing, false).unwrap();

    ui.wait("it to go", |ui| ui.view().selected.as_ref() != Some(&showing));
    assert!(ui.view().selected.is_some(), "the top of what it lists, since the sidebar shows none");
}

#[gpui_kit::test]
fn once_the_workspace_showing_has_gone_the_one_selected_in_its_place_is_one_that_shows(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, _], [_, _, third, last]) = two_projects(cx, &stack);
    ui.click(row(&third));
    ui.click(project_row(&first));

    stack.client().workspace_archive(&third, false).unwrap();

    ui.wait("it to go", |ui| ui.view().selected.as_ref() != Some(&third));
    assert_eq!(ui.view().selected.as_ref(), Some(&last), "the top of what shows, not of the collapsed project above");
}

#[gpui_kit::test]
fn the_keys_pass_over_a_collapsed_projects_workspaces_and_go_on_from_where_the_one_showing_would_be(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let (mut ui, [first, second], made) = two_projects(cx, &stack);
    // Opening a workspace for the first time makes its chat, which counts as working in it, so each is opened before
    // the order is read, and nothing after this changes it.
    for id in &made {
        ui.click(row(id));
        ui.wait("its chat", |ui| ui.view().selected.as_ref() == Some(id) && ui.transcript().is_some());
    }
    ui.hover("send");
    let [upper, lower, third, last]: [WorkspaceId; 4] = order(&mut ui, row, &made).try_into().unwrap();
    ui.click(row(&lower));
    ui.click(project_row(&first));
    assert_eq!(ui.view().selected.as_ref(), Some(&lower), "it's still showing, with its project collapsed");

    ui.press("secondary-]");

    assert_eq!(ui.view().selected.as_ref(), Some(&third), "the first one showing below where its row would be");
    assert!(!ui.has(row(&lower)), "which doesn't bring the project it left back");
    ui.press("secondary-[");
    assert_eq!(ui.view().selected.as_ref(), Some(&last), "around to the last, over the collapsed project's two");

    ui.click(project_row(&first));
    ui.click(project_row(&second));
    assert_eq!(ui.view().selected.as_ref(), Some(&last));
    ui.press("secondary-[");
    assert_eq!(ui.view().selected.as_ref(), Some(&lower), "the last one showing above where its row would be");
    ui.press("secondary-]");
    assert_eq!(ui.view().selected.as_ref(), Some(&upper), "around to the first, over the collapsed project's two");
}

#[gpui_kit::test]
fn a_projects_workspaces_fold_away_under_what_slides_up_over_them_and_come_to_rest(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second], [upper, lower, ..]) = two_projects(cx, &stack);
    let (was, rows) = (top(&mut ui, project_row(&second)), top(&mut ui, row(&upper)));
    let goes = top(&mut ui, project_row(&first)) + px(40.);
    ui.allow_motion();

    ui.click(project_row(&first));

    assert_eq!(top(&mut ui, project_row(&second)), was, "it starts from where it was, since no time has passed");
    assert!(ui.has(row(&upper)) && ui.has(row(&lower)), "so both rows still show");
    ui.play(look::motion::PANEL / 2);
    let halfway = top(&mut ui, project_row(&second));
    assert!(goes < halfway && halfway < was, "{halfway:?} isn't between {goes:?} and {was:?}");
    assert_eq!(top(&mut ui, row(&upper)), rows, "the rows stay where they are");
    assert!(!ui.has(row(&lower)), "and the next project has covered the lower one");
    assert_eq!(ui.view().projects_folding(), 1);
    ui.play(look::motion::PANEL);
    assert_eq!(top(&mut ui, project_row(&second)), goes);
    assert!(!ui.has(row(&upper)));
    assert_eq!(ui.view().projects_folding(), 0, "the fold is over, so its rows are no longer lines");
    assert_eq!(ui.play(look::motion::PANEL), 0, "and nothing asks for another frame");
}

#[gpui_kit::test]
fn a_fold_turned_back_partway_goes_on_from_where_it_had_got_to(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second], [upper, lower, ..]) = two_projects(cx, &stack);
    let was = top(&mut ui, project_row(&second));
    ui.allow_motion();
    ui.click(project_row(&first));
    ui.play(look::motion::PANEL / 8);
    let turned_at = top(&mut ui, project_row(&second));
    assert!(turned_at < was);

    ui.click(project_row(&first));

    assert_eq!(top(&mut ui, project_row(&second)), turned_at, "it doesn't jump");
    ui.play(look::motion::PANEL / 8);
    let back = top(&mut ui, project_row(&second));
    assert!(turned_at < back && back < was, "{back:?} isn't between {turned_at:?} and {was:?}");
    ui.settle(look::motion::PANEL / 4);
    assert_eq!(top(&mut ui, project_row(&second)), was);
    assert!(ui.has(row(&upper)) && ui.has(row(&lower)));
}

/// The list builds the lines drawn in sight, wherever their places are, so what comes after a long project, whose
/// place is far below the window as the project opens, is drawn on its way there.
#[gpui_kit::test]
fn what_comes_after_a_long_project_slides_out_of_sight_as_it_opens_rather_than_going_at_once(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, [first, second], _) = two_projects(cx, &stack);
    let client = stack.client();
    let made = Vec::from_fn(MORE_THAN_FIT, |_| client.workspace_create(&first, None, None, None).unwrap().id);
    // The newest, which is the first project's first row.
    let upper = made.last().unwrap();
    ui.wait("every workspace", |ui| ui.view().visible_workspaces().len() == MORE_THAN_FIT + 4);
    wait_out_setup(&mut ui);
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
