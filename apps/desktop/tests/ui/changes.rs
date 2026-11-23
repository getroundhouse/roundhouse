//! The sidebar of changes, and the workspace's one file tab, which shows whichever file was clicked in it last. The
//! worker reads a workspace's diff, and what the window shows of it, and which tab shows, is the app's, so those open
//! on a fake hub holding a made-up diff, with no worker to wait for. A file written since, and a comment, which the
//! hub keeps, need the real ones.

#![cfg(test)]

use client_core::fake_hub::World;
use desktop::workspace_pane::Showing;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, SharedString, TestAppContext};
use protocol::model::{CommitId, DiffComment, DiffSide, FileContents, GitDiff, SessionStatus, WorkspaceId};

use crate::support::{Diff, FakeHub, Stack, Ui};

/// What git says of the change [`reviewing`] makes.
const PATCH: &str = "diff --git a/a.txt b/a.txt
index 5626abf..814f4a4 100644
--- a/a.txt
+++ b/a.txt
@@ -1 +1,2 @@
 one
+two
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

/// A workspace that changes `a.txt` and adds two files in folders, with its sidebar of changes open.
fn reviewing<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, Diff) {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    let workspace = ui.new_workspace();
    let worktree = ui.worktree(&workspace);
    std::fs::write(worktree.join("a.txt"), "one\ntwo\n").unwrap();
    std::fs::create_dir_all(worktree.join("src/app")).unwrap();
    std::fs::write(worktree.join("src/app/view.rs"), "fn view() {}\n").unwrap();
    std::fs::write(worktree.join("src/lib.rs"), "mod app;\n").unwrap();
    let diff = ui.open_changes();
    // Once the hub's summary has them, which follows them by as long as the worker waits for writes to stop.
    ui.wait("the changes", |ui| ui.has("file-src/lib.rs"));
    (ui, diff)
}

/// [`reviewing`] on a fake hub, which goes with the test, so it's returned to be kept, in one of two workspaces that
/// each make that change and have two chats open, with the other workspace.
fn reviewing_on_a_fake_hub(cx: &mut TestAppContext) -> (FakeHub, Ui<'_>, Diff, WorkspaceId) {
    let mut world = World::default();
    let repo = world.repo("signals");
    let workspaces = ["ledger", "points"].map(|name| world.workspace(&repo, name));
    let base_commit = Some(CommitId::from("3f4c19c8a2d1e0b7f6a5c4d3e2f1a0b9c8d7e6f5"));
    for workspace in &workspaces {
        for _ in 0..2 {
            world.chat(workspace, SessionStatus::Idle, Vec::new());
        }
        let diff = GitDiff { base_commit: base_commit.clone(), patch: PATCH.to_string(), truncated: false };
        world.diffs.insert(workspace.clone(), diff);
    }
    world.base_files.insert("a.txt".to_string(), FileContents::Text { text: "one\n".to_string() });
    let fake = FakeHub::holding(world);
    let mut ui = Ui::open_on(cx, &fake);
    ui.wait("a workspace", |ui| ui.view().selected().is_some() && ui.pane().is_some());
    let showing = ui.view().selected().cloned().unwrap();
    let other = workspaces.into_iter().find(|w| *w != showing).unwrap();
    let diff = ui.open_changes();
    (fake, ui, diff, other)
}

/// Shows `workspace` by its row in the sidebar.
fn show(ui: &mut Ui<'_>, workspace: &WorkspaceId) {
    ui.click(SharedString::from(format!("workspace-{workspace}")));
    ui.wait("the workspace", |ui| ui.view().selected() == Some(workspace) && ui.pane().is_some());
}

fn showing(ui: &Ui<'_>) -> Showing {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.showing())
}

fn file_tab(ui: &Ui<'_>) -> Option<String> {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.file_tab().map(String::from))
}

#[gpui_kit::test]
fn each_file_clicked_shows_in_the_same_file_tab(cx: &mut TestAppContext) {
    let (_hub, mut ui, diff, _) = reviewing_on_a_fake_hub(cx);
    let chats = ui.open_chats();

    ui.open_diff_of(&diff, "a.txt");
    assert_eq!((showing(&ui), file_tab(&ui).as_deref()), (Showing::File, Some("a.txt")));
    ui.open_diff_of(&diff, "src/lib.rs");

    assert_eq!((showing(&ui), file_tab(&ui).as_deref()), (Showing::File, Some("src/lib.rs")));
    assert_eq!(ui.open_chats(), chats, "it's a tab of its own, and opens no chat");
}

#[gpui_kit::test]
fn the_file_tab_stays_behind_a_chat_until_its_closed(cx: &mut TestAppContext) {
    let (_hub, mut ui, diff, _) = reviewing_on_a_fake_hub(cx);
    let chats = ui.open_chats();
    ui.open_diff_of(&diff, "a.txt");

    ui.click(SharedString::from(format!("chat-{}", chats[0])));
    assert_eq!((showing(&ui), file_tab(&ui).as_deref()), (Showing::Chat, Some("a.txt")));
    ui.click("show-file-tab");
    assert_eq!(showing(&ui), Showing::File);

    ui.press("secondary-w");

    assert_eq!((showing(&ui), file_tab(&ui)), (Showing::Chat, None));
    assert!(!ui.has("show-file-tab"));
    assert_eq!(ui.open_chats(), chats, "Cmd-W closed the file tab rather than the chat behind it");
}

/// The file tab is the leftmost tab, so it's in the same place however many chats are open, and Ctrl-Tab comes to it
/// there.
#[gpui_kit::test]
fn the_file_tab_comes_before_the_chats(cx: &mut TestAppContext) {
    let (_hub, mut ui, diff, _) = reviewing_on_a_fake_hub(cx);
    let chats = ui.open_chats();
    assert_eq!(chats.len(), 2);
    ui.open_diff_of(&diff, "a.txt");

    let left = |ui: &mut Ui<'_>, id: SharedString| {
        ui.frame();
        ui.cx.update_window(ui.handle, |_, window, _| window.find(id).bounds().left()).unwrap()
    };
    let file_tab = left(&mut ui, "show-file-tab".into());
    for chat in &chats {
        let chat_tab = left(&mut ui, format!("chat-{chat}").into());
        assert!(file_tab < chat_tab, "the file tab, at {file_tab:?}, is left of chat {chat}, at {chat_tab:?}");
    }

    ui.press("ctrl-tab");
    assert_eq!((showing(&ui), ui.active_chat()), (Showing::Chat, chats[0].clone()), "the first chat follows it");
    ui.press("ctrl-shift-tab");
    assert_eq!(showing(&ui), Showing::File, "and goes back to it");
    ui.show_chat(&chats[1]);
    ui.press("ctrl-tab");
    assert_eq!(showing(&ui), Showing::File, "the last chat wraps around to it");
}

/// The composer isn't drawn while the file tab shows, so coming back to the workspace gives the diff the focus instead,
/// or the window's keys reach nothing.
#[gpui_kit::test]
fn the_window_keys_work_on_the_file_tab_after_coming_back_to_it(cx: &mut TestAppContext) {
    let (_hub, mut ui, diff, other) = reviewing_on_a_fake_hub(cx);
    let reviewed = ui.view().selected().cloned().unwrap();
    ui.open_diff_of(&diff, "a.txt");
    show(&mut ui, &other);

    show(&mut ui, &reviewed);
    assert_eq!(showing(&ui), Showing::File);
    ui.press("ctrl-tab");

    assert_eq!(showing(&ui), Showing::Chat, "Ctrl-Tab moved on from the file tab");
}

#[gpui_kit::test]
fn the_files_show_in_folders_that_close(cx: &mut TestAppContext) {
    let (_hub, mut ui, _, _) = reviewing_on_a_fake_hub(cx);
    assert!(ui.has("file-src/app/view.rs"));
    assert!(!ui.has("folder-src"), "a list until folders are chosen");

    ui.click("changes-layout");
    assert!(ui.has("folder-src") && ui.has("folder-src/app") && ui.has("file-src/app/view.rs"));
    ui.click("folder-src");

    assert!(!ui.has("folder-src/app") && !ui.has("file-src/app/view.rs") && !ui.has("file-src/lib.rs"));
    assert!(ui.has("folder-src") && ui.has("file-a.txt"));
}

#[gpui_kit::test]
fn a_folder_closed_stays_closed_when_a_new_file_splits_its_row(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, _) = reviewing(cx, &stack);
    let worktree = ui.worktree(ui.view().selected().unwrap());
    std::fs::create_dir_all(worktree.join("docs/spec")).unwrap();
    std::fs::write(worktree.join("docs/spec/files.md"), "# Files\n").unwrap();
    ui.click("changes-layout");
    // Choosing Changes again reads the diff again, as a turn's end does, where the hub's summary isn't current.
    ui.click("sidebar-tab-changes");
    ui.wait("the folders, sharing a row", |ui| ui.has("folder-docs/spec"));
    ui.click("folder-docs/spec");
    assert!(!ui.has("file-docs/spec/files.md"));

    std::fs::write(worktree.join("docs/README.md"), "# Docs\n").unwrap();
    ui.click("sidebar-tab-changes");

    ui.wait("the row to be two", |ui| ui.has("folder-docs") && !ui.has("folder-docs/spec"));
    assert!(!ui.has("file-docs/README.md"), "and the folders stay closed");
}

/// Leaves a comment on line 2 of `a.txt`, and returns it as the hub keeps it, once the diff shows it.
fn comment_on_a_txt(ui: &mut Ui<'_>, stack: &Stack, diff: &Diff, text: &str) -> DiffComment {
    let workspace = ui.view().selected().cloned().unwrap();
    ui.open_diff_of(diff, "a.txt");
    ui.start_comment(diff, "a.txt", 2);
    ui.input(text);
    ui.press("enter");
    let mut saved = Vec::new();
    ui.wait("the comment to be saved", |_| {
        saved = stack.client().comment_list(&workspace).unwrap();
        !saved.is_empty()
    });
    let comment = saved.remove(0);
    // The hub has it before the app does, and the diff draws it only once the app has it.
    ui.wait("the comment to show", |ui| ui.has(SharedString::from(format!("edit-{}", comment.id))));
    comment
}

/// So a file with comments can be found from the list, and opened to read them, and a closed folder says it holds one.
#[gpui_kit::test]
fn a_file_with_comments_is_marked_in_the_list_until_theyre_deleted(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = reviewing(cx, &stack);
    let workspace = ui.view().selected().cloned().unwrap();
    assert!(!ui.has("comments-a.txt"));

    let comment = comment_on_a_txt(&mut ui, &stack, &diff, "Why two?");
    stack.client().comment_add(&workspace, "src/lib.rs", DiffSide::New, 1, "And this?", None).unwrap();

    ui.wait("the other comment to be marked", |ui| ui.has("comments-src/lib.rs"));
    assert!(ui.has("comments-a.txt"));
    ui.click("changes-layout");
    assert!(!ui.has("comments-src"), "an open folder leaves it to its files");
    ui.click("folder-src");
    assert!(ui.has("comments-src"), "a closed folder says it holds one");
    ui.click(SharedString::from(format!("delete-{}", comment.id)));
    ui.wait("the mark to go with the comment", |ui| !ui.has("comments-a.txt"));
}

/// Saving a comment takes its box away, and focus with it, so the window's keys still work. Then it's edited where it
/// was left, and Add to Chat copies it, as edited, after what was typed in the composer before it was left.
#[gpui_kit::test]
fn a_comment_saved_leaves_the_keys_working_is_edited_where_it_was_left_and_is_added_to_the_chat(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let (mut ui, diff) = reviewing(cx, &stack);
    let workspace = ui.view().selected().cloned().unwrap();
    ui.input("Keep it short.");
    let comment = comment_on_a_txt(&mut ui, &stack, &diff, "Why two?");

    assert_eq!(showing(&ui), Showing::File);
    ui.press("ctrl-tab");
    assert_eq!(showing(&ui), Showing::Chat, "Ctrl-Tab moved on from the file tab once the comment was saved");
    ui.click("show-file-tab");

    ui.click(SharedString::from(format!("edit-{}", comment.id)));
    let editing = ui.cx.read_entity(&diff.file, |f, cx| f.draft.as_ref().map(|d| d.input.read(cx).text().to_string()));
    assert_eq!(editing.as_deref(), Some("Why two?"), "the comment's text, to edit");
    ui.input(" And not three?");
    ui.press("enter");
    let mut edited = Vec::new();
    ui.wait("the edit to be saved", |_| {
        edited = stack.client().comment_list(&workspace).unwrap();
        edited.len() == 1 && edited[0].id == comment.id && edited[0].text == "Why two? And not three?"
    });
    ui.wait("the edit to show", |ui| ui.comments(&diff).first().is_some_and(|c| c.text == "Why two? And not three?"));

    ui.click(SharedString::from(format!("add-to-chat-{}", comment.id)));
    assert_eq!(showing(&ui), Showing::Chat, "Add to Chat shows the chat");
    ui.frame();
    let added = "Keep it short.\n`a.txt:2`: Why two? And not three?";
    assert_eq!(ui.composer_text(), added, "added after what was typed, on a line of its own");
    // A delete would take it off the diff straight away, before the hub answers.
    let shown: Vec<_> = ui.comments(&diff).into_iter().map(|c| c.id).collect();
    assert_eq!(shown, std::slice::from_ref(&comment.id), "added to the chat, it stays on the diff");
    assert_eq!(stack.client().comment_list(&workspace).unwrap(), edited, "and in the hub");
}

/// Space is pressed as a key, through the window's bindings, as it is when typed, where a binding for it would take it
/// from the comment. What's typed is the app's until it's saved.
#[gpui_kit::test]
fn space_is_typed_in_a_comment(cx: &mut TestAppContext) {
    let (_hub, mut ui, diff, _) = reviewing_on_a_fake_hub(cx);
    ui.open_diff_of(&diff, "a.txt");
    ui.start_comment(&diff, "a.txt", 2);

    ui.input("Why");
    ui.press("space");
    ui.input("two?");

    let typed = ui.cx.read_entity(&diff.file, |f, cx| f.draft.as_ref().map(|d| d.input.read(cx).text().to_string()));
    assert_eq!(typed.as_deref(), Some("Why two?"));
}

#[gpui_kit::test]
fn hiding_the_sidebar_of_changes_hides_it_in_every_workspace(cx: &mut TestAppContext) {
    let (_hub, mut ui, _, other) = reviewing_on_a_fake_hub(cx);
    let first = ui.view().selected().cloned().unwrap();
    show(&mut ui, &other);
    assert!(ui.changes_open(), "open in the first, so open in the second");

    ui.click("toggle-diff");
    ui.press("secondary-[");

    ui.wait("the first workspace", |ui| ui.view().selected() == Some(&first));
    assert!(!ui.changes_open());
    assert!(!ui.has("changed-files"), "the first workspace's changes aren't drawn either");
}
