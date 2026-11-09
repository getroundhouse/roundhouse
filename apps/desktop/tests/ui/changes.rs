//! The sidebar of changes, and the workspace's one diff tab, which shows whichever file was clicked in it last.

#![cfg(test)]

use desktop::workspace_pane::Showing;
use gpui_kit::{AppContext as _, SharedString, TestAppContext};
use protocol::model::DiffComment;

use crate::support::{Diff, Stack, Ui};

/// A workspace that changes `a.txt` and adds two files in folders, with its sidebar of changes open.
fn reviewing<'a>(cx: &'a mut TestAppContext, stack: &Stack) -> (Ui<'a>, Diff) {
    stack.client().repo_add(stack.repo().to_str().unwrap()).unwrap();
    let mut ui = Ui::open(cx, stack);
    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    let workspace = ui.new_workspace();
    let worktree = ui.worktree(&workspace);
    std::fs::write(worktree.join("a.txt"), "one\ntwo\n").unwrap();
    std::fs::create_dir_all(worktree.join("src/app")).unwrap();
    std::fs::write(worktree.join("src/app/view.rs"), "fn view() {}\n").unwrap();
    std::fs::write(worktree.join("src/lib.rs"), "mod app;\n").unwrap();
    let diff = ui.open_diff();
    (ui, diff)
}

fn showing(ui: &Ui<'_>) -> Showing {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.showing())
}

fn diff_tab(ui: &Ui<'_>) -> Option<String> {
    ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.diff_tab().map(String::from))
}

#[gpui_kit::test]
fn each_file_clicked_shows_in_the_same_diff_tab(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = reviewing(cx, &stack);
    let chats = ui.open_chats();

    ui.open_file(&diff, "a.txt");
    assert_eq!((showing(&ui), diff_tab(&ui).as_deref()), (Showing::Diff, Some("a.txt")));
    ui.open_file(&diff, "src/lib.rs");

    assert_eq!((showing(&ui), diff_tab(&ui).as_deref()), (Showing::Diff, Some("src/lib.rs")));
    assert_eq!(ui.open_chats(), chats, "it's a tab of its own, and opens no chat");
}

#[gpui_kit::test]
fn the_diff_tab_stays_behind_a_chat_until_its_closed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = reviewing(cx, &stack);
    let chats = ui.open_chats();
    ui.open_file(&diff, "a.txt");

    ui.click(SharedString::from(format!("chat-{}", chats[0])));
    assert_eq!((showing(&ui), diff_tab(&ui).as_deref()), (Showing::Chat, Some("a.txt")));
    ui.press("ctrl-tab");
    assert_eq!(showing(&ui), Showing::Diff, "it comes after the chats");
    ui.press("ctrl-tab");
    assert_eq!(showing(&ui), Showing::Chat);
    ui.click("show-diff-tab");
    assert_eq!(showing(&ui), Showing::Diff);

    ui.press("cmd-w");

    assert_eq!((showing(&ui), diff_tab(&ui)), (Showing::Chat, None));
    assert!(!ui.has("show-diff-tab"));
    assert_eq!(ui.open_chats(), chats, "Cmd-W closed the diff tab rather than the chat behind it");
}

/// The composer isn't drawn while the diff tab shows, so coming back to the workspace gives the diff the focus instead,
/// or the window's keys reach nothing.
#[gpui_kit::test]
fn the_window_keys_work_on_the_diff_tab_after_coming_back_to_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = reviewing(cx, &stack);
    let reviewed = ui.view().selected.clone().unwrap();
    ui.open_file(&diff, "a.txt");
    ui.new_workspace();

    ui.click(SharedString::from(format!("workspace-{reviewed}")));
    assert_eq!(showing(&ui), Showing::Diff);
    ui.press("ctrl-tab");

    assert_eq!(showing(&ui), Showing::Chat, "Ctrl-Tab moved on from the diff tab");
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

/// Leaves a comment on line 2 of `a.txt`, and returns it as the hub keeps it.
fn comment_on_a_txt(ui: &mut Ui<'_>, stack: &Stack, diff: &Diff, text: &str) -> DiffComment {
    let workspace = ui.view().selected.clone().unwrap();
    ui.open_file(diff, "a.txt");
    ui.start_comment(diff, "a.txt", 2);
    ui.input(text);
    ui.press("enter");
    let mut saved = Vec::new();
    ui.wait("the comment to be saved", |_| {
        saved = stack.client().comment_list(&workspace).unwrap();
        !saved.is_empty()
    });
    saved.remove(0)
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

/// Saving a comment takes its box away, and focus with it, so the window's keys still work.
#[gpui_kit::test]
fn the_window_keys_work_after_a_comment_is_saved(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = reviewing(cx, &stack);
    comment_on_a_txt(&mut ui, &stack, &diff, "Why two?");
    assert_eq!(showing(&ui), Showing::Diff);

    ui.press("ctrl-tab");

    assert_eq!(showing(&ui), Showing::Chat, "Ctrl-Tab moved on from the diff tab");
}

#[gpui_kit::test]
fn add_to_chat_copies_a_comment_into_the_composer(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let (mut ui, diff) = reviewing(cx, &stack);
    let workspace = ui.view().selected.clone().unwrap();
    ui.input("Keep it short.");
    let comment = comment_on_a_txt(&mut ui, &stack, &diff, "Why two?");
    ui.wait("it to show", |ui| ui.comments(&diff).len() == 1);

    ui.click(SharedString::from(format!("add-to-chat-{}", comment.id)));

    assert_eq!(showing(&ui), Showing::Chat);
    ui.frame();
    assert_eq!(ui.composer_text(), "Keep it short.\n`a.txt:2`: Why two?", "after what was typed, on a line of its own");
    // A delete would take it off the diff straight away, before the hub answers.
    let shown: Vec<_> = ui.comments(&diff).into_iter().map(|c| c.id).collect();
    assert_eq!(shown, std::slice::from_ref(&comment.id), "it stays on the diff");
    assert_eq!(stack.client().comment_list(&workspace).unwrap(), [comment], "and in the hub");
}
