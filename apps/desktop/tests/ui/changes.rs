//! The sidebar of changes, and the workspace's one file tab, which shows whichever file was clicked in it last.

#![cfg(test)]

use desktop::workspace_pane::Showing;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, SharedString, TestAppContext};
use protocol::model::DiffComment;

use crate::support::{Diff, Stack, Ui};

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
    (ui, diff)
}

fn showing(ui: &Ui<'_>) -> Showing {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.showing())
}

fn file_tab(ui: &Ui<'_>) -> Option<String> {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.file_tab().map(String::from))
}

#[gpui_kit::test]
fn each_file_clicked_shows_in_the_same_file_tab(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = reviewing(cx, &stack);
    let chats = ui.open_chats();

    ui.open_diff_of(&diff, "a.txt");
    assert_eq!((showing(&ui), file_tab(&ui).as_deref()), (Showing::File, Some("a.txt")));
    ui.open_diff_of(&diff, "src/lib.rs");

    assert_eq!((showing(&ui), file_tab(&ui).as_deref()), (Showing::File, Some("src/lib.rs")));
    assert_eq!(ui.open_chats(), chats, "it's a tab of its own, and opens no chat");
}

#[gpui_kit::test]
fn the_file_tab_stays_behind_a_chat_until_its_closed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = reviewing(cx, &stack);
    let chats = ui.open_chats();
    ui.open_diff_of(&diff, "a.txt");

    ui.click(SharedString::from(format!("chat-{}", chats[0])));
    assert_eq!((showing(&ui), file_tab(&ui).as_deref()), (Showing::Chat, Some("a.txt")));
    ui.click("show-file-tab");
    assert_eq!(showing(&ui), Showing::File);

    ui.press("cmd-w");

    assert_eq!((showing(&ui), file_tab(&ui)), (Showing::Chat, None));
    assert!(!ui.has("show-file-tab"));
    assert_eq!(ui.open_chats(), chats, "Cmd-W closed the file tab rather than the chat behind it");
}

/// The file tab is the leftmost tab, so it's in the same place however many chats are open, and Ctrl-Tab comes to it
/// there.
#[gpui_kit::test]
fn the_file_tab_comes_before_the_chats(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = reviewing(cx, &stack);
    ui.new_chat();
    let chats = ui.open_chats();
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
    let stack = Stack::new();
    let (mut ui, diff) = reviewing(cx, &stack);
    let reviewed = ui.view().selected.clone().unwrap();
    ui.open_diff_of(&diff, "a.txt");
    ui.new_workspace();

    ui.click(SharedString::from(format!("workspace-{reviewed}")));
    assert_eq!(showing(&ui), Showing::File);
    ui.press("ctrl-tab");

    assert_eq!(showing(&ui), Showing::Chat, "Ctrl-Tab moved on from the file tab");
}

#[gpui_kit::test]
fn the_files_show_in_folders_that_close(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, _) = reviewing(cx, &stack);
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
    let worktree = ui.worktree(ui.view().selected.as_ref().unwrap());
    std::fs::create_dir_all(worktree.join("docs/spec")).unwrap();
    std::fs::write(worktree.join("docs/spec/files.md"), "# Files\n").unwrap();
    ui.click("changes-layout");
    // Choosing Changes again reads the diff again, as a turn's end does.
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
    let workspace = ui.view().selected.clone().unwrap();
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

#[gpui_kit::test]
fn a_comment_is_edited_where_it_was_left(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = reviewing(cx, &stack);
    let workspace = ui.view().selected.clone().unwrap();
    let comment = comment_on_a_txt(&mut ui, &stack, &diff, "Why two?");

    ui.click(SharedString::from(format!("edit-{}", comment.id)));
    let editing = ui.cx.read_entity(&diff.file, |f, cx| f.draft.as_ref().map(|d| d.input.read(cx).text().to_string()));
    assert_eq!(editing.as_deref(), Some("Why two?"));
    ui.input(" And not three?");
    ui.press("enter");

    ui.wait("the edit to be saved", |_| {
        let saved = stack.client().comment_list(&workspace).unwrap();
        saved.len() == 1 && saved[0].id == comment.id && saved[0].text == "Why two? And not three?"
    });
    ui.wait("it to show", |ui| ui.comments(&diff).first().is_some_and(|c| c.text == "Why two? And not three?"));
}

/// Space is pressed as a key, through the window's bindings, as it is when typed, where a binding for it would take it
/// from the comment.
#[gpui_kit::test]
fn space_is_typed_in_a_comment(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = reviewing(cx, &stack);
    ui.open_diff_of(&diff, "a.txt");
    ui.start_comment(&diff, "a.txt", 2);

    ui.input("Why");
    ui.press("space");
    ui.input("two?");

    let typed = ui.cx.read_entity(&diff.file, |f, cx| f.draft.as_ref().map(|d| d.input.read(cx).text().to_string()));
    assert_eq!(typed.as_deref(), Some("Why two?"));
}

/// Saving a comment takes its box away, and focus with it, so the window's keys still work.
#[gpui_kit::test]
fn the_window_keys_work_after_a_comment_is_saved(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = reviewing(cx, &stack);
    comment_on_a_txt(&mut ui, &stack, &diff, "Why two?");
    assert_eq!(showing(&ui), Showing::File);

    ui.press("ctrl-tab");

    assert_eq!(showing(&ui), Showing::Chat, "Ctrl-Tab moved on from the file tab");
}

#[gpui_kit::test]
fn add_to_chat_copies_a_comment_into_the_composer(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = reviewing(cx, &stack);
    let workspace = ui.view().selected.clone().unwrap();
    ui.input("Keep it short.");
    let comment = comment_on_a_txt(&mut ui, &stack, &diff, "Why two?");

    ui.click(SharedString::from(format!("add-to-chat-{}", comment.id)));

    assert_eq!(showing(&ui), Showing::Chat);
    ui.frame();
    assert_eq!(ui.composer_text(), "Keep it short.\n`a.txt:2`: Why two?", "after what was typed, on a line of its own");
    // A delete would take it off the diff straight away, before the hub answers.
    let shown: Vec<_> = ui.comments(&diff).into_iter().map(|c| c.id).collect();
    assert_eq!(shown, std::slice::from_ref(&comment.id), "it stays on the diff");
    assert_eq!(stack.client().comment_list(&workspace).unwrap(), [comment], "and in the hub");
}

#[gpui_kit::test]
fn hiding_the_sidebar_of_changes_hides_it_in_every_workspace(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, _) = reviewing(cx, &stack);
    let first = ui.view().selected.clone().unwrap();
    ui.new_workspace();
    assert!(ui.changes_open(), "open in the first, so open in the second");

    ui.click("toggle-diff");
    ui.press("cmd-[");

    ui.wait("the first workspace", |ui| ui.view().selected.as_ref() == Some(&first));
    assert!(!ui.changes_open());
    assert!(!ui.has("changed-files"), "the first workspace's changes aren't drawn either");
}
