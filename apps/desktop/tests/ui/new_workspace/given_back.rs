//! Where a prompt goes when its workspace couldn't be made, or its first chat couldn't be sent it. One whose workspace
//! couldn't be made at all is in `kept.rs`, since the hub keeps it again.

use client_core::FileToSend;
use desktop::new_workspace::Request;
use gpui_kit::{AppContext as _, ClipboardEntry, ClipboardItem, ExternalPaths, TestAppContext};
use protocol::attachment::{MAX_PER_PROMPT, MediaType};
use protocol::capability::settings::Effort;
use protocol::model::AgentKind;
use protocol::prompt::Prompt;

use super::{open_with_a_repo, prompts};
use crate::support::{CLAUDE_STARTS_WITH, Held, Stack, Ui, sidebar_row};

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

/// The hub gives the new chat the prompt only whole. Here it has lost a file's bytes, so it can't copy it, and the
/// prompt stays in the dialog, which opens again to show it with the file, though the workspace was made.
#[gpui_kit::test]
fn a_prompt_the_hub_couldnt_give_the_new_chat_whole_goes_back_to_the_dialog(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    ui.open_new_workspace();
    ui.input("say:with notes");
    ui.paste(copied(&stack, 1));
    ui.wait("the file to be attached", |ui| ui.dialog_attached().len() == 1);
    ui.view().drafts().flushed().recv().unwrap();
    for kept in std::fs::read_dir(stack.hub.paths.attachments_dir.join("workspace-draft.files")).unwrap() {
        std::fs::remove_file(kept.unwrap().path()).unwrap();
    }
    let before = ui.view().records().workspaces().len();

    ui.press("enter");

    ui.wait_for_a_new_workspace(before);
    ui.wait("the dialog again", |ui| ui.has_dialog() && ui.dialog_text() == "say:with notes");
    assert!(ui.view().error.clone().unwrap().contains("Couldn't send that"));
    assert_eq!(ui.dialog_attached(), ["file-0.txt"]);
    assert_eq!(ui.composer_text(), "", "nothing went to the new chat's composer");
}

/// Asks the app to make what `prompt` says, as Create would have, with the files it carries.
fn create(ui: &mut Ui<'_>, prompt: Prompt<FileToSend>) {
    let chosen = (Some(CLAUDE_STARTS_WITH.0), Some(CLAUDE_STARTS_WITH.1));
    let request = Request::new(ui.first_repo_id(), prompt, AgentKind::Claude, chosen, None);
    let view = ui.view.clone();
    ui.cx.update_entity(&view, |v, cx| v.create_workspace(request, cx)).unwrap();
}

/// The dialog can only hold as many files as a prompt carries, so of those given back and those in it since, the last
/// don't fit, and it says so. More than a prompt carries can't come from the dialog, but a request can hold them, and
/// the hub refuses them before making anything, so all of it goes back, and with no text, the hub keeps what fits as
/// what's in the dialog, as it keeps a prompt with text given back. Then the dialog is full, and a prompt given
/// back to it from a workspace that couldn't be made, as if Create had been pressed before those were attached, goes
/// first.
#[gpui_kit::test]
fn files_given_back_to_the_dialog_fit_as_many_as_a_prompt_carries_and_it_says_which_didnt(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    ui.open_new_workspace();
    ui.press("escape");
    let before = ui.view().records().workspaces().len();

    // Files and no text, which the hub keeps as what's in the dialog all the same.
    let too_many = (0..=MAX_PER_PROMPT).map(|i| file(&format!("file-{i}.txt"))).collect();
    create(&mut ui, Prompt::carrying("", too_many));

    ui.wait("too many back in the dialog", |ui| ui.has_dialog() && ui.dialog_attached().len() == MAX_PER_PROMPT);
    let last = format!("file-{MAX_PER_PROMPT}.txt");
    ui.wait("which of too many didn't fit", |ui| ui.view().error.clone().is_some_and(|e| e.contains(&last)));
    assert_eq!(ui.view().records().workspaces().len(), before, "too many: nothing was made");
    ui.wait("the hub to keep those that fit", |_| {
        let kept = stack.client().records().unwrap().new_workspace_draft.unwrap();
        kept.text.is_empty() && kept.files.len() == MAX_PER_PROMPT
    });

    // Emptied of the error, so what comes next is the next case's own.
    ui.press("escape");
    let view = ui.view.clone();
    ui.cx.update(|cx| {
        view.update(cx, |view, cx| {
            view.error = None;
            cx.notify();
        });
    });
    std::fs::remove_dir_all(stack.repo()).unwrap();

    create(&mut ui, Prompt::carrying("say:one more", vec![file("earlier.txt")]));

    ui.wait("one more back in the full dialog", |ui| ui.has_dialog() && ui.dialog_text() == "say:one more");
    let attached = ui.dialog_attached();
    assert_eq!(attached.len(), MAX_PER_PROMPT, "one more: as many as fit");
    assert_eq!(attached[0], "earlier.txt", "what was given back goes first");
    let last = format!("file-{}.txt", MAX_PER_PROMPT - 1);
    assert!(!attached.contains(&last));
    ui.wait("which of the full dialog's didn't fit", |ui| ui.view().error.clone().is_some_and(|e| e.contains(&last)));
}

/// Made from another workspace, as usual, so the new one's pane is made after the prompt comes back. Plan mode goes
/// with it, so the prompt isn't then sent with full access. Its composer is where the prompt goes back to, so the new
/// workspace shows even though another was chosen while it was made, which would otherwise stay showing.
#[gpui_kit::test]
fn a_prompt_the_new_chat_couldnt_be_sent_goes_to_its_composer_which_shows_though_another_was_chosen(
    cx: &mut TestAppContext,
) {
    let stack = Stack::new();
    let mut ui = open_with_a_repo(cx, &stack);
    let first = ui.new_workspace();
    ui.new_workspace();
    let making = Held::checkout(&stack);

    ui.open_new_workspace();
    // A level the hub doesn't know, as a newer app's would be to an older hub, so the workspace and its chat are made
    // and then giving the chat its effort fails, before the prompt is sent.
    ui.cx.update_entity(&ui.dialog(), |d, cx| d.choose_effort(Effort::Unknown, cx));
    ui.paste(copied(&stack, 1));
    ui.wait("the file to be attached", |ui| ui.dialog_attached() == ["file-0.txt"]);
    ui.press("shift-tab");
    ui.input("say:held back");
    ui.press("enter");
    ui.wait("it to start making it", |_| making.runs() == 1);
    ui.click(sidebar_row(&first));
    making.release();

    let made = ui.wait_for_a_new_workspace(2);
    assert_ne!(made, first, "the new one shows");
    ui.wait("the prompt in the new chat's composer", |ui| ui.composer_text() == "say:held back");
    assert_eq!(ui.attached(), ["file-0.txt"]);
    ui.wait("plan mode in the new chat", |ui| ui.chat_mode() == Some(protocol::fixtures::plan_mode()));
    assert!(
        ui.view()
            .error
            .clone()
            .unwrap_or_default()
            .contains("Couldn't give the chat the model, effort and mode chosen")
    );
    assert!(prompts(&ui).is_empty(), "nothing was sent without the effort chosen");
}
