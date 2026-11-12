//! What's in the dialog, which the hub keeps until the workspace is made, files and all, so it's there when the app
//! opens again, and in any other client.

use client_core::FileToSend;
use client_core::records::HubRecords;
use gpui_kit::{AppContext as _, ClipboardEntry, ClipboardItem, ExternalPaths, TestAppContext};
use protocol::attachment::MediaType;
use protocol::event::EventBody;
use protocol::model::{Effort, ModelName, PermissionMode, WorkspaceDraft, WorkspaceDraftToKeep};

use super::open_with_a_repo;
use crate::support::{Frozen, Stack, Ui, wait_until};

/// What the hub keeps of the dialog.
fn kept(stack: &Stack) -> WorkspaceDraft {
    stack.client().records_list().unwrap().workspace_draft.unwrap()
}

/// What another client would keep for the dialog with `text` typed in it.
fn typed_elsewhere(text: &str) -> WorkspaceDraftToKeep {
    WorkspaceDraftToKeep { text: text.to_string(), ..WorkspaceDraftToKeep::default() }
}

fn chosen(ui: &Ui<'_>) -> (ModelName, Effort) {
    ui.cx.read_entity(&ui.dialog(), |d, _| d.chosen())
}

#[gpui_kit::test]
fn whats_in_the_dialog_is_there_when_the_app_opens_again_until_the_workspace_is_made(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    let notes = stack.dir.path().join("notes.txt");
    std::fs::write(&notes, "the first line\n").unwrap();
    ui.open_new_workspace();
    ui.paste(ClipboardItem::from(ClipboardEntry::ExternalPaths(ExternalPaths(vec![notes].into()))));
    ui.wait("the file to be attached", |ui| ui.dialog_attached() == ["notes.txt"]);
    ui.input("half a thought");
    ui.pick_item("new-workspace-model", "Sonnet");
    ui.press("shift-tab");
    ui.press("escape");
    ui.wait("the hub to keep it", |_| {
        let kept = kept(&stack);
        kept.text == "half a thought" && kept.files.len() == 1 && kept.plan
    });

    ui.open_again(&stack);

    ui.wait("the repository", |ui| ui.view().repos.len() == 1);
    ui.open_new_workspace();
    ui.wait("the file it kept", |ui| ui.dialog_attached() == ["notes.txt"]);
    assert_eq!(ui.dialog_text(), "half a thought");
    assert!(ui.dialog_planning());
    assert_eq!(chosen(&ui).0, ModelName::named("sonnet"));
    let before = ui.view().workspaces.len();
    ui.press("enter");
    ui.wait_for_a_new_workspace(before);
    ui.wait("the first prompt, with the file read back", |ui| {
        let carried = ui.events().into_iter().find_map(|e| match e.body {
            EventBody::UserPrompt { attachments, .. } => Some(attachments),
            _ => None,
        });
        carried.is_some_and(|files| files.iter().map(|file| file.name.as_str()).eq(["notes.txt"]))
    });
    ui.wait("making it to empty what the hub keeps", |_| {
        let kept = kept(&stack);
        kept.text.is_empty() && kept.files.is_empty() && !kept.plan
    });
}

#[gpui_kit::test]
fn a_change_made_to_the_dialog_elsewhere_shows_in_it(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    ui.open_new_workspace();
    ui.input("typed here");
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    let elsewhere = stack.client();
    let notes = FileToSend::checked("notes.txt", MediaType::Text, b"a line\n".to_vec()).unwrap();
    let file = notes.attachment.id.clone();
    elsewhere.workspace_draft_file_add(&file, "notes.txt", MediaType::Text, notes.data).unwrap();
    let changed = WorkspaceDraftToKeep {
        files: vec![file],
        model: Some(ModelName::named("sonnet")),
        effort: Some(Effort::High),
        plan: true,
        ..typed_elsewhere("typed elsewhere")
    };

    elsewhere.workspace_draft_keep(&changed).unwrap();

    ui.wait("what was typed elsewhere", |ui| ui.dialog_text() == "typed elsewhere");
    ui.wait("the file attached elsewhere", |ui| ui.dialog_attached() == ["notes.txt"]);
    assert!(ui.dialog_planning());
    assert_eq!(chosen(&ui), (ModelName::named("sonnet"), Effort::High));
    elsewhere.workspace_draft_keep(&typed_elsewhere("")).unwrap();
    ui.wait("the dialog to empty", |ui| ui.dialog_text().is_empty() && ui.dialog_attached().is_empty());
    assert_eq!(ui.cx.read_entity(&ui.dialog(), |d, _| d.mode()), PermissionMode::FullAccess);
}

/// A change made elsewhere while something from here is on its way to the hub waits for it, and then shows, though
/// nothing on the record changes again to say so.
#[gpui_kit::test]
fn a_change_made_to_the_dialog_elsewhere_while_something_from_here_is_on_its_way_shows_once_its_arrived(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    ui.open_new_workspace();
    ui.input("typed here");
    let drafts = ui.view().drafts().unwrap();
    drafts.flushed().recv().unwrap();
    let pid = stack.client().hub_status().unwrap().pid;
    stack.client().workspace_draft_keep(&typed_elsewhere("typed elsewhere")).unwrap();
    // Arrived in client-core, but nothing has run on the main thread since, so the dialog hasn't seen it.
    let records = ui.view().records().unwrap();
    let in_records = || records.read(HubRecords::workspace_draft).map(|draft| draft.text).unwrap_or_default();
    wait_until("client-core to hear of the change", || in_records() == "typed elsewhere");

    let frozen = Frozen::stop(pid);
    let file = FileToSend::checked("notes.txt", MediaType::Text, b"a line".to_vec()).unwrap();
    drafts.attach_to_new_workspace(file);
    let heard = |ui: &Ui<'_>| ui.cx.read_entity(&ui.dialog(), |d, _| d.kept().text.clone());
    ui.wait("the dialog to hear of it", |ui| heard(ui) == "typed elsewhere");
    assert_eq!(ui.dialog_text(), "typed here", "held back while the file is on its way");
    drop(frozen);

    ui.wait("what was typed elsewhere", |ui| ui.dialog_text() == "typed elsewhere");
}
