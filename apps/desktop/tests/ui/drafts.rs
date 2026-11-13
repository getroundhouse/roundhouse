//! What's typed and attached in a chat and not sent, which the hub keeps, so it's there when the app opens again.

#![cfg(test)]

use client_core::{FileToSend, FilesFor};
use desktop::workspace_pane::WorkspacePane;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Context, TestAppContext, Window};
use protocol::attachment::{Attachment, MediaType};
use protocol::event::EventBody;
use protocol::model::{DraftToKeep, SessionId};
use protocol::prompt::Prompt;

use crate::support::{Frozen, Stack, Ui, pencil, sidebar_row, wait_until};

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
            EventBody::UserPrompt { prompt: Prompt { files: attachments, .. }, .. } => Some(attachments),
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

/// A chat that isn't showing takes a change made elsewhere too, and showing it doesn't keep it again, so the change
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
    stack.client().session_draft(&first, &DraftToKeep::new("typed elsewhere")).unwrap();

    ui.show_chat(&first);
    ui.frame();

    ui.wait("what was typed elsewhere", |ui| ui.composer_text() == "typed elsewhere");
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    assert_eq!(kept(&stack, &first), "typed elsewhere");
}

/// As when the phone takes over from the Mac, and back again.
#[gpui_kit::test]
fn a_change_made_elsewhere_shows_in_the_chat_thats_showing(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    ui.input("typed here");
    ui.view().drafts().unwrap().flushed().recv().unwrap();

    stack.client().session_draft(&chat, &DraftToKeep::new("typed elsewhere")).unwrap();

    ui.wait("what was typed elsewhere", |ui| ui.composer_text() == "typed elsewhere");
    ui.input(" and here");
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    assert_eq!(kept(&stack, &chat), "typed elsewhere and here", "typing here carries on from it");
    stack.client().session_draft(&chat, &DraftToKeep::default()).unwrap();
    ui.wait("the composer to empty, as sending it elsewhere does", |ui| ui.composer_text().is_empty());
}

#[gpui_kit::test]
fn a_file_attached_elsewhere_shows_here_until_its_taken_out_there(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    let elsewhere = stack.client();
    let file = FileToSend::checked("build.log", MediaType::Text, b"error at line 42\n".to_vec()).unwrap();
    let id = file.attachment.id.clone();
    elsewhere.attachment_add(&chat, &id, &file.attachment.name, file.attachment.media_type, file.data).unwrap();

    elsewhere.session_draft(&chat, &DraftToKeep { text: "look".to_string(), files: vec![id] }).unwrap();

    ui.wait("the file", |ui| ui.composer_text() == "look" && ui.attached() == ["build.log"]);
    let pane = ui.pane().unwrap();
    ui.cx.update_entity(&pane, |p, cx| p.composer.update(cx, |c, cx| c.set_selected_range(0..0, cx)));
    elsewhere.session_draft(&chat, &DraftToKeep::new("look")).unwrap();
    ui.wait("the file to go", |ui| ui.attached().is_empty());
    assert_eq!(ui.composer_text(), "look");
    let cursor = ui.cx.read_entity(&pane, |p, cx| p.composer.read(cx).cursor());
    assert_eq!(cursor, 0, "what's typed is left as it is, cursor and all");
}

/// Text an input method is still composing isn't handed over until it's done, so the composer can say more than the
/// hub has. A record that changes for something else, as an agent's does as it works, leaves the composer as it is,
/// even in an app opened again, which hasn't kept anything there yet.
#[gpui_kit::test]
fn a_record_that_changes_for_something_else_leaves_the_composer_as_it_is(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    ui.input("typed here");
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    ui.open_again(&stack);
    ui.wait("what was typed", |ui| ui.composer_text() == "typed here");
    // As text being composed is put in, without a change for the pane to hand over.
    let pane = ui.pane().unwrap();
    let composing = |p: &mut WorkspacePane, window: &mut Window, cx: &mut Context<WorkspacePane>| {
        p.composer.update(cx, |c, cx| c.set_value("typed here, and being composed", window, cx));
    };
    ui.cx.update_window(ui.handle, |_, window, cx| pane.update(cx, |p, cx| composing(p, window, cx))).unwrap();

    stack.client().session_rename(&chat, "Renamed elsewhere").unwrap();

    let title = |ui: &Ui<'_>| ui.cx.read_entity(&pane, |p, _| p.session().unwrap().title.clone());
    ui.wait("the pane to hear of it", |ui| title(ui).is_some_and(|title| title.as_str() == "Renamed elsewhere"));
    assert_eq!(ui.composer_text(), "typed here, and being composed");
}

/// A draft changed elsewhere for the chat that shows next is that chat's, so its files come once, though the same
/// change to the records closes the chat that showed.
#[gpui_kit::test]
fn a_draft_changed_elsewhere_as_the_chat_showing_closes_brings_each_file_once(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let showing = ui.active_chat();
    let next = ui.new_chat();
    ui.attach_a_file("build.log", "error at line 42\n");
    ui.input("look");
    ui.view().drafts().unwrap().flushed().recv().unwrap();
    ui.show_chat(&showing);
    let files = kept_files(&stack, &next).into_iter().map(|file| file.id).collect();
    let records = ui.view().feed().unwrap();

    stack.client().session_draft(&next, &DraftToKeep { text: "look at this".to_string(), files }).unwrap();
    stack.client().session_close(&showing).unwrap();
    // Both in client-core before the pane hears of either.
    wait_until("client-core to hear of both", || {
        let sessions: Vec<_> = records.snapshot().every_session().cloned().collect();
        // Or gone, as a chat nothing was sent in goes as it closes.
        let closed = !sessions.iter().any(|s| s.id == showing && s.closed_at_ms.is_none());
        closed && sessions.iter().any(|s| s.id == next && s.draft.text == "look at this")
    });

    ui.wait("the change, in the chat that shows next", |ui| {
        ui.active_chat() == next && ui.composer_text() == "look at this"
    });
    assert_eq!(ui.attached(), ["build.log"]);
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

/// A change made elsewhere while something from here is on its way to the hub waits for it, and then shows, though
/// nothing on the chat's record changes again to say so. That's when the hub tells of a change made elsewhere before
/// its reply to one of this app's drafts kept just before, which a test can't make it do, so a file handed over alone
/// stands in for that draft: either way, something from here is on its way as the pane hears of the change.
#[gpui_kit::test]
fn a_change_made_elsewhere_while_something_from_here_is_on_its_way_shows_once_its_arrived(cx: &mut TestAppContext) {
    let stack = Stack::new();
    let mut ui = Ui::open_with_a_repository(cx, &stack);
    ui.new_workspace();
    let chat = ui.active_chat();
    ui.input("typed here");
    let drafts = ui.view().drafts().unwrap();
    drafts.flushed().recv().unwrap();
    let pid = stack.client().hub_status().unwrap().pid;
    stack.client().session_draft(&chat, &DraftToKeep::new("typed elsewhere")).unwrap();
    // Arrived in client-core, but nothing has run on the main thread since, so the pane hasn't seen it.
    let records = ui.view().feed().unwrap();
    let draft_in =
        |records: &client_core::RecordsFeed| records.snapshot().session(&chat).map_or_default(|s| s.draft.text.clone());
    wait_until("client-core to hear of the change", || draft_in(&records) == "typed elsewhere");

    let frozen = Frozen::stop(pid);
    let file = FileToSend::checked("notes.txt", MediaType::Text, b"a line".to_vec()).unwrap();
    drafts.attach(FilesFor::Chat(chat), file);
    let heard = |ui: &Ui<'_>| ui.cx.read_entity(&ui.pane().unwrap(), |p, _| p.session().unwrap().draft.text.clone());
    ui.wait("the pane to hear of it", |ui| heard(ui) == "typed elsewhere");
    assert_eq!(ui.composer_text(), "typed here", "held back while the file is on its way");
    drop(frozen);

    ui.wait("what was typed elsewhere", |ui| ui.composer_text() == "typed elsewhere");
}
