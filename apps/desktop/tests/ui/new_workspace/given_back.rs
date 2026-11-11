//! Where a prompt goes when its workspace couldn't be made, or its first chat couldn't be sent it, and the focus around
//! the dialog while workspaces show behind it.

use client_core::FileToSend;
use desktop::new_workspace::Request;
use gpui_kit::{AppContext as _, ClipboardEntry, ClipboardItem, ExternalPaths, TestAppContext};
use protocol::attachment::{MAX_PER_PROMPT, MediaType};
use protocol::model::{AgentKind, DEFAULT_EFFORT, DEFAULT_MODEL, Effort, PermissionMode, WorkspaceStatus};

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
    ui.press("shift-tab");
    ui.input("say:never sent");
    ui.press("enter");

    ui.wait("the failure", |ui| ui.view().error.is_some() && ui.view().busy.is_free());
    assert!(ui.view().error.clone().unwrap().contains("couldn't create the workspace"));
    assert!(ui.view().workspaces.is_empty());
    assert!(ui.has_dialog(), "the dialog opens again to show where the prompt went");
    assert_eq!(ui.dialog_text(), "say:never sent");
    assert!(ui.dialog_planning(), "plan mode comes back with the prompt");
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
        model: DEFAULT_MODEL,
        effort: DEFAULT_EFFORT,
        permission_mode: PermissionMode::FullAccess,
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

/// Made from another workspace, as usual, so the new one's pane is made after the prompt comes back. Plan mode goes
/// with it, so the prompt isn't then sent with full access.
#[gpui_kit::test]
fn a_prompt_the_new_chat_couldnt_be_sent_goes_to_its_composer_with_its_files_and_plan_mode(cx: &mut TestAppContext) {
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
    ui.press("shift-tab");
    ui.input("say:held back");
    ui.press("enter");

    ui.wait_for_a_new_workspace(before);
    ui.wait("the prompt in the new chat's composer", |ui| ui.composer_text() == "say:held back");
    assert_eq!(ui.attached(), ["file-0.txt"]);
    ui.wait("plan mode in the new chat", |ui| ui.chat_mode() == Some(PermissionMode::Plan));
    assert!(
        ui.view()
            .error
            .clone()
            .unwrap_or_default()
            .contains("couldn't give the chat the model, effort and mode chosen")
    );
    assert!(prompts(&ui).is_empty(), "nothing was sent without the effort chosen");
}

/// More files than a prompt carries can't come from the dialog, but a request can hold them, and the hub refuses them.
#[gpui_kit::test]
fn a_prompt_that_couldnt_be_sent_goes_to_the_new_chats_composer_with_the_files_that_fit(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    let before = ui.view().workspaces.len();
    let files: Vec<_> = (0..=MAX_PER_PROMPT).map(|i| file(&format!("file-{i}.txt"))).collect();
    let (model, effort) = (DEFAULT_MODEL, DEFAULT_EFFORT);
    let (repo, prompt, permission_mode) = (ui.first_repo_id(), "say:too many".to_string(), PermissionMode::FullAccess);
    let request = Request { repo, prompt, files, model, effort, permission_mode };

    let view = ui.view.clone();
    ui.cx.update_entity(&view, |v, cx| v.create_workspace(request, cx)).unwrap();

    ui.wait_for_a_new_workspace(before);
    ui.wait("the prompt in the new chat's composer", |ui| ui.composer_text() == "say:too many");
    assert_eq!(ui.attached().len(), MAX_PER_PROMPT);
    assert!(prompts(&ui).is_empty());
    let last = format!("file-{MAX_PER_PROMPT}.txt");
    assert!(ui.view().error.clone().is_some_and(|e| e.contains(&last)), "it says which file didn't fit");
}

/// Made elsewhere, so the app has no pane for it until it's selected, as with a workspace made while the dialog is
/// open. A new pane brings its chat to the front, which would focus its composer.
#[gpui_kit::test]
fn typing_in_the_dialog_stays_there_when_another_workspace_shows_behind_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    ui.new_workspace();
    let client = stack.client();
    let other = client.workspace_create(&ui.first_repo_id(), None, None, None).unwrap().id;
    client.session_ensure(&other, AgentKind::Claude).unwrap();
    ui.wait("the other workspace, ready", |ui| {
        ui.view().workspaces.iter().any(|w| w.id == other && w.status == WorkspaceStatus::Ready)
    });

    ui.open_new_workspace();
    ui.input("next");
    // As a workspace being made is selected once it's ready.
    let view = ui.view.clone();
    ui.cx.update_entity(&view, |v, cx| v.select(other.clone(), cx));
    ui.wait("its pane", |ui| ui.transcript().is_some() && ui.view().selected.as_ref() == Some(&other));
    ui.input(" task");

    assert_eq!(ui.dialog_text(), "next task");
    assert_eq!(ui.composer_text(), "");
    ui.press("escape");
    ui.input("here");
    assert_eq!(ui.composer_text(), "here", "closing the dialog focuses the workspace showing");
}
