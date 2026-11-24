//! Which of its two tabs the sidebar of changes shows: All files in a workspace that changes nothing, and Changes in
//! one that changes something, from its first change on. `desktop::sidebar_tabs` tests the rule, and these that the
//! pane follows it.

#![cfg(test)]

use desktop::review::Listing;
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
    ui.cx.read_entity(&review, |r, _| r.listing().map(Listing::changed))
}

/// While the hub keeps the workspace's changes current, a tab clicked in a workspace that changes nothing gives way to
/// Changes at the workspace's first change, made outside a turn, and one clicked after that stays through later
/// changes.
#[gpui_kit::test]
fn a_tab_clicked_gives_way_to_the_first_change_and_one_clicked_after_it_stays(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, workspace) = in_a_new_workspace(cx, &stack);
    let review = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.diff.as_ref().map(|d| d.review.clone())).unwrap();
    ui.wait("the hub's summary", |ui| ui.cx.read_entity(&review, |r, _| r.listing().is_some()));
    ui.click("sidebar-tab-all-files");
    assert_eq!(tab(&ui), Some(SidebarTab::AllFiles));

    std::fs::write(ui.worktree(&workspace).join("new.txt"), "new\n").unwrap();

    ui.wait("the first change", |ui| changed(ui) == Some(1));
    assert_eq!(tab(&ui), Some(SidebarTab::Changes), "the first change shows Changes");
    ui.click("sidebar-tab-all-files");
    std::fs::write(ui.worktree(&workspace).join("newer.txt"), "newer\n").unwrap();
    ui.wait("the later change", |ui| changed(ui) == Some(2));
    assert_eq!(tab(&ui), Some(SidebarTab::AllFiles), "a later change leaves the tab clicked");
}

/// Each workspace shows the tab that suits it, so one that changes something isn't on All files because another was.
#[gpui_kit::test]
fn each_workspace_starts_on_all_files_and_shows_changes_from_its_own_first_change_on(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, first) = in_a_new_workspace(cx, &stack);
    assert_eq!(tab(&ui), Some(SidebarTab::AllFiles), "a new workspace changes nothing");

    ui.have_the_agent_run("echo new > new.txt");

    ui.wait("its change", |ui| ui.has("file-new.txt"));
    assert_eq!(tab(&ui), Some(SidebarTab::Changes), "its first change shows Changes");
    ui.new_workspace();
    ui.wait("the new workspace's files", |ui| ui.has("all-file-a.txt"));
    assert_eq!(tab(&ui), Some(SidebarTab::AllFiles), "it changes nothing");
    ui.click(gpui_kit::SharedString::from(format!("workspace-{first}")));

    ui.wait("the first workspace's changes", |ui| ui.has("file-new.txt"));
    assert_eq!(tab(&ui), Some(SidebarTab::Changes));
}
