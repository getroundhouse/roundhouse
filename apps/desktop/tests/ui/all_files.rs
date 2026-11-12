//! All files in the sidebar of changes, and a file opened from it, which the workspace's one file tab shows as it is.

#![cfg(test)]

use client_core::Client;
use desktop::app::Connection;
use desktop::file_view::FileView;
use desktop::workspace_pane::{Opened, Showing};
use gpui_kit::{AppContext as _, Entity, SharedString, TestAppContext};
use protocol::model::SessionStatus;

use crate::support::{Stack, Ui};

/// A workspace that adds two files in folders to the repository's `a.txt`, and a folder git ignores that holds only a
/// folder, with All files showing them.
fn browsing<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    browse(cx, stack)
}

/// The app open on a workspace like [`browsing`]'s, in a repository that's already added.
fn browse<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> Ui<'a> {
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let workspace = ui.new_workspace();
    let worktree = ui.worktree(&workspace);
    std::fs::create_dir_all(worktree.join("src/app")).unwrap();
    std::fs::write(worktree.join("src/app/view.rs"), "fn view() {}\n").unwrap();
    std::fs::write(worktree.join("src/lib.rs"), "mod app;\n").unwrap();
    std::fs::create_dir_all(worktree.join("build/out")).unwrap();
    std::fs::write(worktree.join("build/out/app.txt"), "built\n").unwrap();
    std::fs::write(worktree.join(".gitignore"), "build/\n").unwrap();
    show_all_files(&mut ui);
    ui
}

fn show_all_files(ui: &mut Ui<'_>) {
    ui.click("sidebar-tab-all-files");
    ui.wait("All files to list them", |ui| ui.has("all-file-a.txt"));
}

fn viewer(ui: &Ui<'_>) -> Entity<FileView> {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.diff.as_ref().map(|d| d.viewer.clone())).unwrap()
}

fn shown_text(ui: &Ui<'_>) -> Option<String> {
    ui.cx.read_entity(&viewer(ui), |viewer, _| viewer.shown_text())
}

/// The file the diff shows, and the file the file view shows.
fn views(ui: &Ui<'_>) -> (Option<String>, Option<String>) {
    let diff = ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.diff.as_ref().map(|d| d.file.clone())).unwrap();
    let diffed = ui.cx.read_entity(&diff, |f, _| f.path.as_ref().map(ToString::to_string));
    (diffed, ui.cx.read_entity(&viewer(ui), |v, _| v.path.as_ref().map(ToString::to_string)))
}

/// What's in front under the tabs, the file tab's file, and what it shows of it.
fn tab(ui: &Ui<'_>) -> (Showing, Option<String>, Option<Opened>) {
    let pane = ui.pane().unwrap();
    ui.cx.read_entity(&pane, |p, _| (p.showing(), p.file_tab().map(String::from), p.file_tab_opened()))
}

/// Opens `src/lib.rs` from All files, and waits for its text to show.
fn open_lib(ui: &mut Ui<'_>) {
    ui.click("all-folder-src");
    ui.click("all-file-src/lib.rs");
    ui.wait("the file to show", |ui| shown_text(ui).as_deref() == Some("mod app;"));
}

#[gpui_kit::test]
fn all_files_lists_the_worktree_in_folders_that_start_closed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = browsing(cx, &stack);
    assert!(ui.has("all-folder-src") && !ui.has("all-file-src/lib.rs"));

    ui.click("all-folder-src");

    assert!(ui.has("all-folder-src/app") && ui.has("all-file-src/lib.rs"));
    assert!(!ui.has("all-file-src/app/view.rs"), "a folder inside stays closed");
    assert!(!ui.has("changed-files"), "in place of the changes");
}

#[gpui_kit::test]
fn a_file_clicked_in_all_files_shows_as_it_is_in_the_file_tab(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = browsing(cx, &stack);
    let chats = ui.open_chats();

    open_lib(&mut ui);
    assert_eq!(tab(&ui), (Showing::File, Some("src/lib.rs".to_string()), Some(Opened::File)));
    assert_eq!(ui.open_chats(), chats, "it's the file tab, and opens no chat");
    ui.click("sidebar-tab-changes");
    ui.wait("the changes", |ui| ui.has("file-a.txt") || ui.has("file-src/lib.rs"));
    ui.click("file-src/lib.rs");

    assert_eq!(tab(&ui), (Showing::File, Some("src/lib.rs".to_string()), Some(Opened::Diff)), "the same tab");
    assert_eq!(views(&ui), (Some("src/lib.rs".to_string()), None), "the diff in place of the file");
}

/// Either one takes the other's place in the one tab, whichever comes first.
#[gpui_kit::test]
fn a_file_clicked_in_all_files_takes_the_place_of_a_diff(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = browsing(cx, &stack);
    ui.click("sidebar-tab-changes");
    ui.wait("the changes", |ui| ui.has("file-src/lib.rs"));
    ui.click("file-src/lib.rs");
    assert_eq!(tab(&ui).2, Some(Opened::Diff));
    show_all_files(&mut ui);

    open_lib(&mut ui);

    assert_eq!(tab(&ui), (Showing::File, Some("src/lib.rs".to_string()), Some(Opened::File)));
    assert_eq!(views(&ui), (None, Some("src/lib.rs".to_string())), "the file in place of the diff");
}

/// A folder git ignores whole, such as `target`, can hold hundreds of thousands of files, so it's read as it opens.
#[gpui_kit::test]
fn a_folder_git_ignores_is_read_as_it_opens(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = browsing(cx, &stack);
    assert!(ui.has("all-folder-build") && !ui.has("all-folder-build/out"));

    ui.click("all-folder-build");
    ui.wait("what's in it", |ui| ui.has("all-folder-build/out"));
    assert!(ui.has("all-folder-build"), "it keeps its row, rather than sharing one with the folder in it");
    ui.click("all-folder-build/out");
    ui.wait("what's in that", |ui| ui.has("all-file-build/out/app.txt"));
    ui.click("all-file-build/out/app.txt");

    ui.wait("the file to show", |ui| shown_text(ui).as_deref() == Some("built"));
}

/// The composer isn't drawn while the file tab shows a file, so coming back to the workspace gives the file the focus,
/// or the window's keys reach nothing.
#[gpui_kit::test]
fn the_window_keys_work_on_a_file_after_coming_back_to_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = browsing(cx, &stack);
    let browsed = ui.view().selected.clone().unwrap();
    open_lib(&mut ui);
    ui.new_workspace();

    ui.click(SharedString::from(format!("workspace-{browsed}")));
    assert_eq!(tab(&ui).0, Showing::File);
    ui.press("ctrl-tab");

    assert_eq!(tab(&ui).0, Showing::Chat, "Ctrl-Tab moved on from the file");
}

/// The agent changes files as it works, so the end of its turn reads again what's shown of them.
#[gpui_kit::test]
fn a_turn_s_end_reads_the_files_and_the_one_shown_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = browsing(cx, &stack);
    open_lib(&mut ui);
    let chat = ui.active_chat();
    ui.show_chat(&chat);

    ui.have_the_agent_run("echo '// more' >> src/lib.rs && echo new > new.txt");
    ui.wait("the turn to end", |ui| ui.session_status() == Some(SessionStatus::Idle) && ui.has("all-file-new.txt"));

    ui.wait("the file to show what changed", |ui| shown_text(ui).as_deref() == Some("mod app;\n// more"));
}

/// Choosing All files again lists the files again, as a turn's end does, without running a turn.
fn list_again(ui: &mut Ui<'_>, file: &str) {
    ui.click("sidebar-tab-all-files");
    ui.wait("the files to be listed again", |ui| ui.has(SharedString::from(format!("all-file-{file}"))));
}

#[gpui_kit::test]
fn a_folder_open_stays_open_when_a_new_file_splits_its_row(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = browsing(cx, &stack);
    let worktree = ui.worktree(ui.view().selected.as_ref().unwrap());
    std::fs::create_dir_all(worktree.join("docs/spec")).unwrap();
    std::fs::write(worktree.join("docs/spec/files.md"), "# Files\n").unwrap();
    ui.click("sidebar-tab-all-files");
    ui.wait("the folders, sharing a row", |ui| ui.has("all-folder-docs/spec"));
    ui.click("all-folder-docs/spec");
    assert!(ui.has("all-file-docs/spec/files.md"));

    std::fs::write(worktree.join("docs/README.md"), "# Docs\n").unwrap();
    list_again(&mut ui, "docs/README.md");

    assert!(ui.has("all-folder-docs") && ui.has("all-folder-docs/spec"), "the row is two now");
    assert!(ui.has("all-file-docs/spec/files.md"), "and both stay open");
}

#[gpui_kit::test]
fn a_folder_left_open_inside_a_closed_one_is_read_again_when_it_shows_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = browsing(cx, &stack);
    let worktree = ui.worktree(ui.view().selected.as_ref().unwrap());
    ui.click("all-folder-build");
    ui.wait("what's in it", |ui| ui.has("all-folder-build/out"));
    ui.click("all-folder-build/out");
    ui.wait("what's in that", |ui| ui.has("all-file-build/out/app.txt"));
    ui.click("all-folder-build");

    std::fs::write(worktree.join("build/out/later.txt"), "later\n").unwrap();
    ui.click("all-folder-build");

    ui.wait("what's in it now", |ui| ui.has("all-file-build/out/later.txt"));
}

#[gpui_kit::test]
fn listing_again_reads_again_each_folder_open_inside_one_read_as_it_opens(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = browsing(cx, &stack);
    let worktree = ui.worktree(ui.view().selected.as_ref().unwrap());
    ui.click("all-folder-build");
    ui.wait("what's in it", |ui| ui.has("all-folder-build/out"));
    ui.click("all-folder-build/out");
    ui.wait("what's in that", |ui| ui.has("all-file-build/out/app.txt"));

    std::fs::write(worktree.join("build/out/later.txt"), "later\n").unwrap();

    list_again(&mut ui, "build/out/later.txt");
}

/// A folder opened, or a file clicked, just as the hub goes can't be read only because the connection has gone, so the
/// folder stays open and the file isn't said to be unreadable, and both show what's in them once the hub's back.
#[gpui_kit::test]
fn what_s_opened_as_the_hub_goes_shows_once_it_s_back(cx: &mut TestAppContext) {
    let mut stack = Stack::new();
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let daemon = stack.without_a_daemon();
    let mut ui = browse(cx, &stack);
    ui.click("all-folder-src");
    let all_files =
        ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.diff.as_ref().map(|d| d.all_files.clone())).unwrap();

    ui.click_and_leave("all-folder-build");
    ui.click_and_leave("all-file-src/lib.rs");
    client_core::stop_hub(&stack.hub.paths.socket).unwrap();
    ui.wait("the app to find no hub", |ui| matches!(ui.view().connection, Connection::Failed(_)));

    assert_eq!(ui.view().error.as_deref(), None);
    assert!(ui.cx.read_entity(&all_files, |a, _| a.is_open("build")), "the folder stays open");
    assert_eq!(ui.cx.read_entity(&viewer(&ui), |v, _| v.failure()), None);
    Client::connect_or_start(&stack.hub.paths, &daemon, "test").unwrap();
    ui.click("try-hub-again");
    ui.wait("what's in the folder", |ui| ui.has("all-folder-build/out"));
    ui.wait("the file", |ui| shown_text(ui).as_deref() == Some("mod app;"));
}
