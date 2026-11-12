//! What's typed and attached in a chat's composer and not sent, which the hub keeps for whoever opens the chat next.

#![cfg(test)]

use client_core::RecordsFeed;
use client_core::records::HubRecords;
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
