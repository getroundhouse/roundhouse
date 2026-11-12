//! What's typed and attached in a chat's composer and not sent, which the hub keeps for whoever opens the chat next.

#![cfg(test)]

use client_core::records::HubRecords;
use client_core::{Drafts, FileToSend, RecordsFeed};
use protocol::attachment::MediaType;
use protocol::model::{Draft, NewDraft, SessionId, SessionStatus};

use crate::support::{Stack, wait_until};

/// What client-core's copy says is typed in this session.
fn draft_in(feed: &RecordsFeed, id: &SessionId) -> Option<String> {
    feed.read(|records: &HubRecords| {
        records.sessions().values().flatten().find(|s| &s.id == id).map(|s| s.draft.text.clone())
    })
}

#[test]
fn a_draft_is_kept_across_a_restart_and_every_client_hears_of_it() {
    let mut stack = Stack::start();
    let client = stack.client();
    let feed = RecordsFeed::start(&client);
    let (_, session) = stack.workspace(&client);

    let kept = client.session_draft(&session.id, &NewDraft::new("half a thought")).unwrap();

    assert_eq!(kept.draft.text, "half a thought");
    assert_eq!(kept.status, SessionStatus::Idle, "keeping a draft doesn't start the agent");
    wait_until("the feed to say what's typed", || draft_in(&feed, &session.id).as_deref() == Some("half a thought"));
    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    assert_eq!(client.session_get(&session.id).unwrap().draft.text, "half a thought");
    assert_eq!(client.session_draft(&session.id, &NewDraft::default()).unwrap().draft, Draft::default());
}

#[test]
fn what_a_client_types_is_kept_once_it_quits() {
    let stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let drafts = Drafts::start(&client);

    for text in ["h", "ha", "half", "half a", "half a thought"] {
        drafts.typed(&session.id, NewDraft::new(text));
    }
    drafts.flushed().recv().unwrap();

    assert_eq!(client.session_get(&session.id).unwrap().draft.text, "half a thought");
}

#[test]
fn a_drafts_files_are_kept_across_a_restart_until_its_let_go_of_them() {
    let mut stack = Stack::start();
    let client = stack.client();
    let (_, session) = stack.workspace(&client);
    let drafts = Drafts::start(&client);
    let log = FileToSend::checked("build.log", MediaType::Text, b"error at line 42\n".to_vec()).unwrap();
    let never_given = FileToSend::checked("other.log", MediaType::Text, b"a line\n".to_vec()).unwrap();

    drafts.attach(&session.id, log.clone());
    let files = vec![log.attachment.id.clone(), never_given.attachment.id.clone()];
    drafts.set(&session.id, NewDraft { text: "look".to_string(), files });
    drafts.flushed().recv().unwrap();

    stack.kill_hub();
    stack.start_hub();
    let client = stack.client();
    let (id, file) = (&session.id, &log.attachment.id);
    let kept = client.session_get(id).unwrap().draft;
    assert_eq!(kept.files, std::slice::from_ref(&log.attachment), "the hub leaves out one it wasn't given");
    assert_eq!(client.attachment_read(id, file).unwrap(), log.data);
    client.attachment_remove(id, file).unwrap();
    assert!(client.attachment_read(id, file).is_ok(), "the hub keeps a file the draft names");
    client.session_draft(id, &NewDraft::new("look")).unwrap();
    client.attachment_remove(id, file).unwrap();
    assert!(client.attachment_read(id, file).is_err());
}
