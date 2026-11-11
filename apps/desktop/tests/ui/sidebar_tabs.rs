//! Which of its two tabs the sidebar of changes shows: All files in a workspace that changes nothing, and Changes in
//! one that changes something, from its first change on. `desktop::sidebar_tabs` tests the rule, and these that the
//! pane follows it.

#![cfg(test)]

use desktop::sidebar_tabs::SidebarTab;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::WorkspaceId;

use crate::support::{Stack, Ui};

fn tab(ui: &Ui<'_>) -> Option<SidebarTab> {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.sidebar_tab())
}

/// The app open on a new workspace, which changes nothing, with All files listing its one file.
fn in_a_new_workspace<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, WorkspaceId) {
    let mut ui = Ui::open_with_a_repository(cx, stack);
    let workspace = ui.new_workspace();
    ui.wait("All files to list them", |ui| ui.has("all-file-a.txt"));
    (ui, workspace)
}

/// How many files the selected workspace changes, once its changes are read.
fn changed(ui: &Ui<'_>) -> Option<usize> {
    let review = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.diff.as_ref().map(|d| d.review.clone()))?;
    ui.cx.read_entity(&review, |r, _| r.files.as_ref().map(Vec::len))
}

#[gpui_kit::test]
fn a_workspace_that_changes_nothing_starts_on_all_files(cx: &mut TestAppContext) {
    let stack = Stack::new();

    let (ui, _) = in_a_new_workspace(cx, &stack);

    assert_eq!(tab(&ui), Some(SidebarTab::AllFiles));
}

#[gpui_kit::test]
fn a_workspace_s_first_change_shows_changes(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, _) = in_a_new_workspace(cx, &stack);

    ui.have_the_agent_run("echo new > new.txt");

    ui.wait("its change", |ui| ui.has("file-new.txt"));
    assert_eq!(tab(&ui), Some(SidebarTab::Changes));
}

/// A change made outside a turn is found when a tab's click reads the changes again, and the tab clicked is the one
/// that shows.
#[gpui_kit::test]
fn a_tab_clicked_shows_even_when_its_click_finds_the_first_change(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, workspace) = in_a_new_workspace(cx, &stack);
    std::fs::write(ui.worktree(&workspace).join("new.txt"), "new\n").unwrap();

    ui.click("sidebar-tab-all-files");

    ui.wait("the new file, and the change counted", |ui| ui.has("all-file-new.txt") && changed(ui) == Some(1));
    assert_eq!(tab(&ui), Some(SidebarTab::AllFiles));
}

/// Each workspace shows the tab that suits it, so one that changes something isn't on All files because another was.
#[gpui_kit::test]
fn each_workspace_has_its_own_tab(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, first) = in_a_new_workspace(cx, &stack);
    ui.have_the_agent_run("echo new > new.txt");
    ui.wait("its change", |ui| ui.has("file-new.txt"));

    ui.new_workspace();
    ui.wait("the new workspace's files", |ui| ui.has("all-file-a.txt"));
    assert_eq!(tab(&ui), Some(SidebarTab::AllFiles), "it changes nothing");
    ui.click(gpui_kit::SharedString::from(format!("workspace-{first}")));

    ui.wait("the first workspace's changes", |ui| ui.has("file-new.txt"));
    assert_eq!(tab(&ui), Some(SidebarTab::Changes));
}
