//! Where a prompt goes when its workspace couldn't be made, or its first chat couldn't be sent it, and the focus around
//! the dialog while workspaces show behind it.

use client_core::FileToSend;
use desktop::new_workspace::Request;
use desktop::workspace_pane::Busy;
use gpui_kit::{AppContext as _, ClipboardEntry, ClipboardItem, ExternalPaths, TestAppContext};
use protocol::attachment::{MAX_PER_PROMPT, MediaType};
use protocol::model::Effort;

use super::{open_with_a_repo, prompts};
use crate::support::Stack;

/// A text file named `name`, as the dialog would read it.
fn file(name: &str) -> FileToSend {
    FileToSend::checked(name, MediaType::Text, b"a line\n".to_vec()).unwrap()
}

/// `count` files written to the test's directory, copied as Finder copies them.
fn copied(stack: &Stack, count: usize) -> ClipboardItem {
    let paths = (0..count).map(|i| {
        let path = stack.dir.path().join(format!("file-{i}.txt"));
        std::fs::write(&path, "a line\n").unwrap();
        path
    });
    ClipboardItem::from(ClipboardEntry::ExternalPaths(ExternalPaths(paths.collect())))
}

#[gpui_kit::test]
fn a_workspace_that_cant_be_made_gives_the_prompt_back_to_the_dialog(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    std::fs::remove_dir_all(stack.repo()).unwrap();

    ui.open_new_workspace();
    ui.input("say:never sent");
    ui.press("enter");

    ui.wait("the failure", |ui| ui.view().error.is_some() && ui.view().busy == Busy::Free);
    assert!(ui.view().error.clone().unwrap().contains("couldn't create the workspace"));
    assert!(ui.view().workspaces.is_empty());
    assert!(ui.has_dialog(), "the dialog opens again to show where the prompt went");
    assert_eq!(ui.dialog_text(), "say:never sent");
}

/// The dialog can only hold as many files as a prompt carries, so of those given back and those attached since, the
/// last don't fit, and it says so.
#[gpui_kit::test]
fn files_given_back_to_a_full_dialog_go_first_and_it_says_which_didnt_fit(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    ui.open_new_workspace();
    ui.paste(copied(&stack, MAX_PER_PROMPT));
    ui.wait("the files to be attached", |ui| ui.dialog_attached().len() == MAX_PER_PROMPT);
    ui.press("escape");
    std::fs::remove_dir_all(stack.repo()).unwrap();

    // As if Create had been pressed before these were attached, and making the workspace then failed.
    let request = Request {
        repo: ui.first_repo_id(),
        prompt: "say:one more".to_string(),
        files: vec![file("earlier.txt")],
        model: None,
        effort: None,
    };
    let view = ui.view.clone();
    ui.cx.update_entity(&view, |v, cx| v.create_workspace(request, cx)).unwrap();

    ui.wait("the prompt back in the dialog", |ui| ui.has_dialog() && ui.dialog_text() == "say:one more");
    let attached = ui.dialog_attached();
    assert_eq!(attached.len(), MAX_PER_PROMPT);
    assert_eq!(attached[0], "earlier.txt");
    let last = format!("file-{}.txt", MAX_PER_PROMPT - 1);
    assert!(!attached.contains(&last));
    ui.wait("what didn't fit", |ui| ui.view().error.clone().is_some_and(|e| e.contains(&last)));
}

/// Made from another workspace, as usual, so the new one's pane is made after the prompt comes back.
#[gpui_kit::test]
fn a_prompt_the_new_chat_couldnt_be_sent_goes_to_its_composer_with_its_files(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    ui.new_workspace();
    let before = ui.view().workspaces.len();

    ui.open_new_workspace();
    // A level the hub doesn't know, as a newer app's would be to an older hub, so the workspace and its chat are made
    // and then giving the chat its effort fails, before the prompt is sent.
    ui.cx.update_entity(&ui.dialog(), |d, cx| d.choose_effort(Effort::Unknown, cx));
    ui.paste(copied(&stack, 1));
    ui.wait("the file to be attached", |ui| ui.dialog_attached() == ["file-0.txt"]);
    ui.input("say:held back");
    ui.press("enter");

    ui.wait_for_a_new_workspace(before);
    ui.wait("the prompt in the new chat's composer", |ui| ui.composer_text() == "say:held back");
    assert_eq!(ui.attached(), ["file-0.txt"]);
    assert!(ui.view().error.clone().unwrap_or_default().contains("couldn't give the chat its model and effort"));
    assert!(prompts(&ui).is_empty(), "nothing was sent without the effort chosen");
}

/// More files than a prompt carries can't come from the dialog, but a request can hold them, and the hub refuses them.
#[gpui_kit::test]
fn a_prompt_that_couldnt_be_sent_goes_to_the_new_chats_composer_with_the_files_that_fit(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    let before = ui.view().workspaces.len();
    let files: Vec<_> = (0..=MAX_PER_PROMPT).map(|i| file(&format!("file-{i}.txt"))).collect();
    let request =
        Request { repo: ui.first_repo_id(), prompt: "say:too many".to_string(), files, model: None, effort: None };

    let view = ui.view.clone();
    ui.cx.update_entity(&view, |v, cx| v.create_workspace(request, cx)).unwrap();

    ui.wait_for_a_new_workspace(before);
    ui.wait("the prompt in the new chat's composer", |ui| ui.composer_text() == "say:too many");
    assert_eq!(ui.attached().len(), MAX_PER_PROMPT);
    assert!(prompts(&ui).is_empty());
    let last = format!("file-{MAX_PER_PROMPT}.txt");
    assert!(ui.view().error.clone().is_some_and(|e| e.contains(&last)), "it says which file didn't fit");
}

#[gpui_kit::test]
fn typing_in_the_dialog_stays_there_when_another_workspace_shows_behind_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    let first = ui.new_workspace();
    ui.new_workspace();

    ui.open_new_workspace();
    ui.input("next");
    // As a workspace being made is selected once it's ready.
    let view = ui.view.clone();
    ui.cx.update_entity(&view, |v, cx| v.select(first.clone(), cx));
    ui.input(" task");

    assert_eq!(ui.view().selected.as_ref(), Some(&first));
    assert_eq!(ui.dialog_text(), "next task");
    assert_eq!(ui.composer_text(), "");
    ui.press("escape");
    ui.input("here");
    assert_eq!(ui.composer_text(), "here", "closing the dialog focuses the workspace showing");
}
