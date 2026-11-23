//! All files in the sidebar of changes, and a file opened from it, which the workspace's one file tab shows as it is.
//! The worker lists and reads a worktree, and what the window shows of it is the app's, so those open on a fake hub
//! holding a made-up worktree, with no worker to wait for. What's read again once the worktree changes, and what's
//! opened as the hub goes, need the real ones.

#![cfg(test)]

use std::fmt::Write as _;

use client_core::fake_hub::World;
use desktop::app::Connection;
use desktop::file_view::FileView;
use desktop::workspace_pane::{Opened, Showing};
use gpui_kit::{AppContext as _, Entity, SharedString, TestAppContext};
use protocol::model::{FileContents, GitDiff, SessionStatus, WorkspaceId, WorktreeFiles};

use crate::support::{FakeHub, Stack, Ui};

/// What git says of the change [`browse`] makes, which adds its files but for the folder git ignores.
const PATCH: &str = "diff --git a/.gitignore b/.gitignore
new file mode 100644
index 0000000..1e2f3a4
--- /dev/null
+++ b/.gitignore
@@ -0,0 +1 @@
+build/
diff --git a/src/app/view.rs b/src/app/view.rs
new file mode 100644
index 0000000..2a1b3c4
--- /dev/null
+++ b/src/app/view.rs
@@ -0,0 +1 @@
+fn view() {}
diff --git a/src/lib.rs b/src/lib.rs
new file mode 100644
index 0000000..5d6e7f8
--- /dev/null
+++ b/src/lib.rs
@@ -0,0 +1 @@
+mod app;
";

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
    // Found by the hub's summary of the changes, which a click on All files doesn't wait for, so only once they're
    // found is All files the tab clicked after the workspace's first change.
    ui.wait("the changes", |ui| ui.has("file-src/lib.rs"));
    show_all_files(&mut ui);
    ui
}

/// [`browsing`] on a fake hub, which goes with the test, so it's returned to be kept, in one of two workspaces that
/// each make that change, with the other workspace.
fn browsing_on_a_fake_hub(cx: &mut TestAppContext) -> (FakeHub, Ui<'_>, WorkspaceId) {
    let strings = |names: &[&str]| Vec::from_iter(names.iter().map(ToString::to_string));
    let listed = |paths: &[&str], folders: &[&str]| WorktreeFiles {
        paths: strings(paths),
        folders: strings(folders),
        truncated: false,
    };
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspaces = ["ledger", "points"].map(|name| world.workspace(&repo, name));
    for workspace in &workspaces {
        world.chat(workspace, SessionStatus::Idle, Vec::new());
        let diff = GitDiff { base_commit: None, patch: PATCH.to_string(), truncated: false };
        world.diffs.insert(workspace.clone(), diff);
    }
    world.worktree =
        listed(&[".gitignore", "a.txt", "long.md", "notes.md", "src/app/view.rs", "src/lib.rs"], &["build"]);
    world.worktree_folders.insert("build".to_string(), listed(&[], &["build/out"]));
    world.worktree_folders.insert("build/out".to_string(), listed(&["build/out/app.txt"], &[]));
    let files = [("src/lib.rs", "mod app;\n".to_string()), ("build/out/app.txt", "built\n".to_string())];
    let files = files.into_iter().chain([("notes.md", NOTES.to_string()), ("long.md", paragraphs(LONG))]);
    for (path, text) in files {
        world.worktree_files.insert(path.to_string(), FileContents::Text { text });
    }
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("a workspace", |ui| ui.view().selected().is_some() && ui.pane().is_some());
    let showing = ui.view().selected().cloned().unwrap();
    let other = workspaces.into_iter().find(|w| *w != showing).unwrap();
    show_all_files(&mut ui);
    (fake, ui, other)
}

/// A Markdown file in the fake hub's worktree.
const NOTES: &str = "# Notes\n\nThe **sidebar** lists them.\n";

/// How many paragraphs the fake hub's `long.md` has, far more than the window shows.
const LONG: usize = 300;

/// `count` paragraphs of Markdown, each saying which it is.
fn paragraphs(count: usize) -> String {
    (1..=count).fold(String::new(), |mut text, ix| {
        _ = write!(text, "Paragraph **{ix}**.\n\n");
        text
    })
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
    let (_hub, mut ui, _) = browsing_on_a_fake_hub(cx);
    assert!(ui.has("all-folder-src") && !ui.has("all-file-src/lib.rs"));

    ui.click("all-folder-src");

    assert!(ui.has("all-folder-src/app") && ui.has("all-file-src/lib.rs"));
    assert!(!ui.has("all-file-src/app/view.rs"), "a folder inside stays closed");
    assert!(!ui.has("changed-files"), "in place of the changes");
}

#[gpui_kit::test]
fn a_file_clicked_in_all_files_shows_as_it_is_in_the_file_tab(cx: &mut TestAppContext) {
    let (_hub, mut ui, _) = browsing_on_a_fake_hub(cx);
    let chats = ui.open_chats();

    open_lib(&mut ui);
    assert_eq!(tab(&ui), (Showing::File, Some("src/lib.rs".to_string()), Some(Opened::File)));
    assert_eq!(ui.open_chats(), chats, "it's the file tab, and opens no chat");
    ui.click("sidebar-tab-changes");
    ui.wait("the changes", |ui| ui.has("file-src/lib.rs"));
    ui.click("file-src/lib.rs");

    assert_eq!(tab(&ui), (Showing::File, Some("src/lib.rs".to_string()), Some(Opened::Diff)), "the same tab");
    assert_eq!(views(&ui), (Some("src/lib.rs".to_string()), None), "the diff in place of the file");
}

/// Either one takes the other's place in the one tab, whichever comes first.
#[gpui_kit::test]
fn a_file_clicked_in_all_files_takes_the_place_of_a_diff(cx: &mut TestAppContext) {
    let (_hub, mut ui, _) = browsing_on_a_fake_hub(cx);
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
    let (_hub, mut ui, _) = browsing_on_a_fake_hub(cx);
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
    let (_hub, mut ui, other) = browsing_on_a_fake_hub(cx);
    let browsed = ui.view().selected().cloned().unwrap();
    open_lib(&mut ui);
    ui.click(SharedString::from(format!("workspace-{other}")));
    ui.wait("the other workspace", |ui| ui.view().selected() == Some(&other) && ui.pane().is_some());

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

/// Each step writes what only it waits for: listing again reads again each folder open inside one read as it opens,
/// a folder open stays open when a new file splits its row, and a folder left open inside a closed one is read again
/// when it shows again.
#[gpui_kit::test]
fn what_s_open_in_all_files_is_read_again_and_stays_open_as_the_worktree_changes(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = browsing(cx, &stack);
    let worktree = ui.worktree(ui.view().selected().unwrap());
    ui.click("all-folder-build");
    ui.wait("what's in the ignored folder", |ui| ui.has("all-folder-build/out"));
    ui.click("all-folder-build/out");
    ui.wait("what's in the folder in that", |ui| ui.has("all-file-build/out/app.txt"));

    std::fs::write(worktree.join("build/out/later.txt"), "later\n").unwrap();
    std::fs::create_dir_all(worktree.join("docs/spec")).unwrap();
    std::fs::write(worktree.join("docs/spec/files.md"), "# Files\n").unwrap();
    list_again(&mut ui, "build/out/later.txt");

    ui.wait("the new folders, sharing a row", |ui| ui.has("all-folder-docs/spec"));
    ui.click("all-folder-docs/spec");
    assert!(ui.has("all-file-docs/spec/files.md"));
    std::fs::write(worktree.join("docs/README.md"), "# Docs\n").unwrap();
    list_again(&mut ui, "docs/README.md");
    assert!(ui.has("all-folder-docs") && ui.has("all-folder-docs/spec"), "the split row is two now");
    assert!(ui.has("all-file-docs/spec/files.md"), "and both folders of the split row stay open");

    ui.click("all-folder-build");
    std::fs::write(worktree.join("build/out/again.txt"), "again\n").unwrap();
    ui.click("all-folder-build");
    ui.wait("what's in the folder left open, now", |ui| ui.has("all-file-build/out/again.txt"));
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
    ui.wait("the app to find no hub", |ui| matches!(ui.view().connection(), Connection::Failed(_)));

    assert_eq!(ui.view().error.as_deref(), None);
    assert!(ui.cx.read_entity(&all_files, |a, _| a.is_open("build")), "the folder stays open");
    assert_eq!(ui.cx.read_entity(&viewer(&ui), |v, _| v.failure()), None);
    local_hub::connect_or_start(&stack.hub.paths, &daemon, "test").unwrap();
    ui.click("try-hub-again");
    ui.wait("what's in the folder", |ui| ui.has("all-folder-build/out"));
    ui.wait("the file", |ui| shown_text(ui).as_deref() == Some("mod app;"));
}

/// A Markdown file opens rendered, as the chat draws Markdown, so a copy has its words and not its marks, and Source
/// shows its lines as any file's, until Preview is chosen again.
#[gpui_kit::test]
fn a_markdown_file_opens_rendered_and_source_shows_its_lines(cx: &mut TestAppContext) {
    let (_fake, mut ui, _) = browsing_on_a_fake_hub(cx);

    ui.click("all-file-notes.md");
    ui.wait("the file, rendered", |ui| rendered(ui).as_deref() == Some("Notes\n\nThe sidebar lists them."));
    ui.click("markdown-source");

    assert_eq!(rendered(&ui), None);
    assert_eq!(shown_text(&ui).as_deref(), Some("# Notes\n\nThe **sidebar** lists them."));
    ui.click("markdown-preview");
    ui.wait("the file, rendered again", |ui| rendered(ui).is_some());
}

/// A long Markdown file is drawn as far as the window shows, and a copy of all of it has the blocks it hasn't drawn
/// too.
#[gpui_kit::test]
fn a_long_markdown_file_draws_what_s_in_sight_and_copies_all_of_it(cx: &mut TestAppContext) {
    let (_fake, mut ui, _) = browsing_on_a_fake_hub(cx);

    ui.click("all-file-long.md");

    let copied = (1..=LONG).map(|ix| format!("Paragraph {ix}.")).collect::<Vec<_>>().join("\n\n");
    ui.wait("the file, rendered", |ui| rendered(ui).is_some_and(|rendered| rendered == copied));
    let shown = ui.texts_shown();
    assert!(shown.contains("Paragraph 1. | Paragraph 2."), "{shown}");
    assert!(!shown.contains(&format!("Paragraph {LONG}.")), "{shown}");
}

fn rendered(ui: &Ui<'_>) -> Option<String> {
    ui.cx.read_entity(&viewer(ui), FileView::rendered)
}
