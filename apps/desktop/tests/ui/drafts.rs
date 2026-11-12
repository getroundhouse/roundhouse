//! What's typed and attached in a chat and not sent, which the hub keeps, so it's there when the app opens again.

#![cfg(test)]

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::attachment::Attachment;
use protocol::event::EventBody;
use protocol::model::{NewDraft, SessionId};

use crate::support::{Stack, Ui, pencil, sidebar_row};

/// What the hub keeps of what's typed in the chat.
fn kept(stack: &Stack, chat: &SessionId) -> String {
    stack.client().session_get(chat).unwrap().draft.text
}

/// The files the hub keeps with the chat's draft.
fn kept_files(stack: &Stack, chat: &SessionId) -> Vec<Attachment> {
    stack.client().session_get(chat).unwrap().draft.files
}

fn names(files: &[Attachment]) -> Vec<&str> {
    files.iter().map(|file| file.name.as_str()).collect()
}

#[gpui_kit::test]
fn whats_typed_is_there_when_the_app_opens_again_until_its_sent(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let typed_in = ui.new_workspace();
    let chat = ui.active_chat();
    ui.input("half a thought");
    ui.wait("the hub to keep it", |_| kept(&stack, &chat) == "half a thought");
    // Made last, so it's the one that shows when the app opens again.
    let newest = ui.new_workspace();

    ui.open_again(&stack);

    ui.wait("the newest workspace", |ui| ui.view().selected.as_ref() == Some(&newest));
    assert!(ui.visible(pencil(&typed_in)), "its row says so before it's opened");
    ui.click(sidebar_row(&typed_in));
    ui.wait("what was typed", |ui| ui.composer_text() == "half a thought");
    ui.input(" and more");
    assert_eq!(ui.composer_text(), "half a thought and more", "it's ready to say more");
    ui.press("enter");
    ui.wait("sending it to empty what's kept", |_| kept(&stack, &chat).is_empty());
}

#[gpui_kit::test]
fn whats_attached_is_there_when_the_app_opens_again_until_its_sent(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    ui.attach_a_file("build.log", "error at line 42\n");
    ui.wait("the hub to keep it", |_| names(&kept_files(&stack, &chat)) == ["build.log"]);

    ui.open_again(&stack);

    ui.wait("the file to come back", |ui| ui.pane().is_some() && ui.attached() == ["build.log"]);
    ui.input("say:look");
    ui.press("enter");
    ui.wait("sending it to empty what's kept", |_| kept_files(&stack, &chat).is_empty());
    // The composer, and so what's kept, empties as the prompt goes, before the hub has recorded it.
    let mut carried = None;
    ui.wait("the prompt", |ui| {
        carried = ui.events().into_iter().find_map(|e| match e.body {
            EventBody::UserPrompt { attachments, .. } => Some(attachments),
            _ => None,
        });
        carried.is_some()
    });
    assert_eq!(names(&carried.unwrap()), ["build.log"], "the prompt carries it");
}

#[gpui_kit::test]
fn a_file_taken_out_of_the_composer_is_let_go_of(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    ui.attach_a_file("build.log", "error at line 42\n");
    let mut kept = Vec::new();
    ui.wait("the hub to keep it", |_| {
        kept = kept_files(&stack, &chat);
        !kept.is_empty()
    });

    ui.remove_attached(&kept[0].id);

    assert_eq!(ui.attached(), Vec::<String>::new());
    ui.wait("the hub to let go of it", |_| stack.client().attachment_read(&chat, &kept[0].id).is_err());
    assert_eq!(kept_files(&stack, &chat), []);
}

/// As what's typed in it does. A chat nothing was sent in goes altogether, so this one has a prompt in it.
#[gpui_kit::test]
fn whats_attached_in_a_chat_goes_when_its_closed(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let closing = ui.new_chat();
    ui.input("say:hi");
    ui.press("enter");
    ui.wait_idle_with("the answer", |text| text.contains("hi"));
    ui.attach_a_file("build.log", "error at line 42\n");
    let mut kept = Vec::new();
    ui.wait("the hub to keep it", |_| {
        kept = kept_files(&stack, &closing);
        !kept.is_empty()
    });

    ui.close_from_tab(&closing);

    ui.wait("the hub to let go of it", |_| stack.client().attachment_read(&closing, &kept[0].id).is_err());
    assert_eq!(kept_files(&stack, &closing), []);
    // The hub can be done before the app hears the chat has closed.
    ui.wait("another chat to show", |ui| ui.active_chat() != closing);
    assert_eq!(ui.attached(), Vec::<String>::new(), "it doesn't go to the chat that shows instead");
}

/// Showing a chat puts back what this app had of it without keeping it again, so a change made to it elsewhere since
/// isn't undone.
#[gpui_kit::test]
fn showing_a_chat_doesnt_undo_a_change_made_to_its_draft_elsewhere(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let first = ui.active_chat();
    ui.input("typed here");
    ui.new_chat();
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    assert_eq!(kept(&stack, &first), "typed here");
    stack.client().session_draft(&first, &NewDraft::new("typed elsewhere")).unwrap();

    ui.show_chat(&first);
    ui.frame();

    ui.view().drafts().unwrap().flushed().recv().unwrap();
    assert_eq!(kept(&stack, &first), "typed elsewhere");
}

/// The hub keeps a chat's files as they're read back, so what's typed meanwhile is kept with them.
#[gpui_kit::test]
fn whats_typed_while_a_chats_files_come_back_is_kept_with_them(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    let drafted = ui.new_workspace();
    let chat = ui.active_chat();
    ui.attach_a_file("build.log", "error at line 42\n");
    ui.input("half");
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    // Made last, so it's the one that shows when the app opens again, and the other's files come back as it's opened.
    ui.new_workspace();
    ui.open_again(&stack);

    // Opened and typed in before anything else runs, so its file is still on its way back.
    ui.cx.update_window(ui.handle, |_, window, cx| window.click(sidebar_row(&drafted), cx)).unwrap();
    ui.cx.update_window(ui.handle, |_, window, cx| window.input(" a thought", cx)).unwrap();
    assert_eq!((ui.composer_text(), ui.attached()), ("half a thought".to_string(), vec![]));

    ui.view().drafts().unwrap().flushed().recv().unwrap();
    assert_eq!(kept(&stack, &chat), "half a thought");
    assert_eq!(names(&kept_files(&stack, &chat)), ["build.log"], "the draft goes on naming the file");
    ui.wait("the file to come back", |ui| ui.attached() == ["build.log"]);
}

/// Closing a chat takes what the hub kept of its composer, and a file that was still on its way there goes too.
#[gpui_kit::test]
fn a_file_still_on_its_way_as_its_chat_closes_is_let_go_of_too(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let closing = ui.new_chat();
    ui.input("say:hi");
    ui.press("enter");
    ui.wait_idle_with("the answer", |text| text.contains("hi"));
    // Large, so it's still on its way as the chat closes.
    ui.attach_a_file("build.log", &"error at line 42\n".repeat(1_000_000));

    ui.close_from_tab(&closing);

    let on_disk = stack.hub.paths.attachments_dir.join(closing.as_str());
    let files_left = || std::fs::read_dir(&on_disk).map_or(0, Iterator::count);
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    ui.wait("the hub to let go of it", |_| files_left() == 0);
}
