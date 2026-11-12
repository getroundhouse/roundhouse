//! What's in the dialog, which the hub keeps until the workspace is made, files and all, so it's there when the app
//! opens again, and in any other client.

use client_core::FileToSend;
use gpui_kit::{AppContext as _, ClipboardEntry, ClipboardItem, ExternalPaths, TestAppContext};
use protocol::attachment::MediaType;
use protocol::event::EventBody;
use protocol::model::{Effort, ModelName, NewWorkspaceDraft, NewWorkspaceDraftToKeep, PermissionMode};
use protocol::prompt::Prompt;

use super::open_with_a_repo;
use crate::support::{Frozen, Stack, Ui, wait_until};

/// What the hub keeps of the dialog.
fn kept(stack: &Stack) -> NewWorkspaceDraft {
    stack.client().records_list().unwrap().new_workspace_draft.unwrap()
}

/// What another client would keep for the dialog with `text` typed in it.
fn typed_elsewhere(text: &str) -> NewWorkspaceDraftToKeep {
    NewWorkspaceDraftToKeep { text: text.to_string(), ..NewWorkspaceDraftToKeep::default() }
}

fn chosen(ui: &Ui<'_>) -> (ModelName, Effort) {
    ui.cx.read_entity(&ui.dialog(), |d, _| d.chosen()).expect("the hub describes the agent the first chat runs")
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

    ui.wait("the repository", |ui| ui.view().records().repos().len() == 1);
    ui.open_new_workspace();
    ui.wait("the file it kept", |ui| ui.dialog_attached() == ["notes.txt"]);
    assert_eq!(ui.dialog_text(), "half a thought");
    assert!(ui.dialog_planning());
    assert_eq!(chosen(&ui).0, ModelName::named("sonnet"));
    let before = ui.view().records().workspaces().len();
    ui.press("enter");
    ui.wait_for_a_new_workspace(before);
    ui.wait("the first prompt, with the file read back", |ui| {
        let carried = ui.events().into_iter().find_map(|e| match e.body {
            EventBody::UserPrompt { prompt: Prompt { files: attachments, .. }, .. } => Some(attachments),
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
    elsewhere.new_workspace_draft_file_add(&file, "notes.txt", MediaType::Text, notes.data).unwrap();
    let changed = NewWorkspaceDraftToKeep {
        files: vec![file],
        model: Some(ModelName::named("sonnet")),
        effort: Some(Effort::High),
        plan: true,
        ..typed_elsewhere("typed elsewhere")
    };

    elsewhere.new_workspace_draft_keep(&changed).unwrap();

    ui.wait("what was typed elsewhere", |ui| ui.dialog_text() == "typed elsewhere");
    ui.wait("the file attached elsewhere", |ui| ui.dialog_attached() == ["notes.txt"]);
    assert!(ui.dialog_planning());
    assert_eq!(chosen(&ui), (ModelName::named("sonnet"), Effort::High));
    elsewhere.new_workspace_draft_keep(&typed_elsewhere("")).unwrap();
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
    stack.client().new_workspace_draft_keep(&typed_elsewhere("typed elsewhere")).unwrap();
    // Arrived in client-core, but nothing has run on the main thread since, so the dialog hasn't seen it.
    let records = ui.view().feed().unwrap();
    let in_records = || records.snapshot().new_workspace_draft().map(|draft| draft.text.clone()).unwrap_or_default();
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

/// Has the dialog take a record kept elsewhere, of a prompt carrying a file, which it hasn't read back yet. A read is a
/// task, and a test's tasks run only once it lets them, so the dialog takes the record here, and what the test does
/// next comes before the read can run.
fn kept_elsewhere_with_a_file(ui: &mut Ui<'_>, stack: &Stack) {
    ui.open_new_workspace();
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    let elsewhere = stack.client();
    let notes = FileToSend::checked("notes.txt", MediaType::Text, b"a line\n".to_vec()).unwrap();
    let file = notes.attachment.id.clone();
    elsewhere.new_workspace_draft_file_add(&file, "notes.txt", MediaType::Text, notes.data).unwrap();
    elsewhere
        .new_workspace_draft_keep(&NewWorkspaceDraftToKeep { files: vec![file], ..typed_elsewhere("say:with notes") })
        .unwrap();
    let records = ui.view().feed().unwrap();
    let in_records =
        || records.snapshot().new_workspace_draft().filter(|draft| draft.text == "say:with notes").cloned();
    wait_until("client-core to hear of the change", || in_records().is_some());
    let (dialog, record) = (ui.dialog(), in_records().unwrap());
    ui.cx
        .update_window(ui.handle, |_, window, cx| dialog.update(cx, |d, cx| d.take_kept_now(record, window, cx)))
        .unwrap();
}

/// Create doesn't wait while the files the hub kept are read back, as the hub makes the workspace from what it keeps,
/// and sends them itself.
#[gpui_kit::test]
fn files_the_hub_kept_go_with_the_prompt_before_theyre_read_back(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    kept_elsewhere_with_a_file(&mut ui, &stack);
    let before = ui.view().records().workspaces().len();

    ui.press("enter");

    assert!(!ui.has_dialog(), "Create didn't wait");
    ui.wait_for_a_new_workspace(before);
    let carried = |ui: &Ui<'_>| {
        let prompt = ui.events().into_iter().find_map(|e| match e.body {
            EventBody::UserPrompt { prompt: Prompt { files: attachments, .. }, .. } => Some(attachments),
            _ => None,
        });
        prompt.map(|files| files.into_iter().map(|file| file.name).collect::<Vec<_>>())
    };
    ui.wait("the prompt", |ui| carried(ui).is_some());
    assert_eq!(carried(&ui), Some(vec!["notes.txt".to_string()]));
    ui.wait("the dialog emptied on the hub", |_| kept(&stack).text.is_empty() && kept(&stack).files.is_empty());
    assert!(ui.dialog_attached().is_empty(), "the file wasn't brought back once it had gone");
}

/// One that wasn't read back before Create was pressed is read back with the prompt, which the dialog shows again.
#[gpui_kit::test]
fn files_the_hub_kept_come_back_to_the_dialog_when_the_workspace_cant_be_made(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    kept_elsewhere_with_a_file(&mut ui, &stack);
    std::fs::remove_dir_all(stack.repo()).unwrap();

    ui.press("enter");

    ui.wait("the failure", |ui| ui.view().error.is_some() && ui.view().busy.is_free());
    assert!(ui.view().records().workspaces().is_empty());
    assert_eq!(ui.dialog_text(), "say:with notes");
    ui.wait("the file read back", |ui| ui.dialog_attached() == ["notes.txt"]);
    assert_eq!(kept(&stack).files.iter().map(|file| file.name.as_str()).collect::<Vec<_>>(), ["notes.txt"]);
}

#[gpui_kit::test]
fn a_prompt_given_back_to_the_dialog_is_kept_on_the_hub_again(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    std::fs::remove_dir_all(stack.repo()).unwrap();
    ui.open_new_workspace();
    ui.input("say:never sent");

    ui.press("enter");

    ui.wait("the failure", |ui| ui.view().error.is_some() && ui.view().busy.is_free());
    assert_eq!(ui.dialog_text(), "say:never sent");
    ui.wait("the hub to keep it again", |_| kept(&stack).text == "say:never sent");
}
