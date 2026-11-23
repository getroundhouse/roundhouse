//! What the queue takes, and what a change to it does, until a prompt in it is sent.

use protocol::attachment::{AttachmentBytes, MediaType};
use protocol::capability::Control;
use protocol::client::{ErrorCode, HubNotice};
use protocol::event::{EventBody, TurnOutcome};
use protocol::model::SessionStatus;
use protocol::prompt::Prompt;
use protocol::worker::WorkerRequest;

use super::turns::{Chat, ids, sent_at};
use crate::attachment_files::FileFor;
use crate::store::AddedBy;

/// Adds a file to the chat for a prompt to carry. An image is enough of a PNG for anything that only looks at how it
/// starts, which is all the hub does.
async fn add(chat: &Chat, id: &str, name: &str, media_type: MediaType) -> protocol::attachment::Attachment {
    let data = if media_type == MediaType::Png {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.resize(64, 7);
        png
    } else {
        b"error at line 42".to_vec()
    };
    let (kept_for, data) = (FileFor::Session(chat.id.clone()), AttachmentBytes::new(data));
    chat.hub.file_add(kept_for, id.into(), name.into(), media_type, data, AddedBy::Client).await.unwrap()
}

#[tokio::test]
async fn a_queued_prompt_changes_or_goes_until_its_sent_and_not_after() {
    let dir = tempfile::tempdir().unwrap();
    let chat = Chat::at_work(dir.path()).await;
    chat.queue("q1", Prompt::new("before")).await.unwrap();
    chat.queue("q2", Prompt::new("taken out")).await.unwrap();

    let changed = chat.change("q1", "after").await.unwrap().queued;
    let left = chat.hub.session_unqueue(chat.id.clone(), "q2".into()).await.unwrap().queued;
    let taken_out = chat.change("q2", "changed once it was out").await.map_err(|e| e.code);

    assert_eq!(changed[0].prompt.text, "after", "a change keeps its place");
    assert_eq!(ids(&left), ["q1"]);
    assert_eq!(taken_out.err(), Some(ErrorCode::NotFound), "out, so a change doesn't queue it again");
    chat.finishes(None, TurnOutcome::Success).await;
    let sent = chat.given(2).await[1];
    chat.finishes(Some(sent), TurnOutcome::Success).await;
    let late = chat.change("q1", "too late").await.map_err(|e| e.code);
    assert_eq!(late.err(), Some(ErrorCode::Conflict), "sent, so it can't change");
    assert!(chat.record().queued.is_empty());
    let events = chat.log();
    let prompt = events.iter().find(|e| e.seq == sent).map(|e| &e.body);
    assert!(matches!(prompt, Some(EventBody::UserPrompt { prompt, .. }) if prompt.text == "after"), "{prompt:?}");
    assert_eq!(sent_at(&events, "q2"), None, "and the one taken out never was");
}

/// Queuing a prompt is when the person last did something in its chat's workspace, which the feed says then, and the
/// hub sending it later, with nobody there, isn't.
#[tokio::test]
async fn a_queued_prompt_moves_its_workspace_as_its_queued_and_not_again_as_its_sent() {
    let dir = tempfile::tempdir().unwrap();
    let chat = Chat::at_work(dir.path()).await;
    let mut feed = chat.hub.subscribe_feed();

    let queued = chat.queue("q1", Prompt::new("after")).await.unwrap().user_active_at_ms;

    // Said by the time the queue answers, rather than with its pull request, which the hub reads once the turn the
    // prompt waits on ends.
    let told = loop {
        match feed.try_recv().map(std::sync::Arc::unwrap_or_clone) {
            Ok(HubNotice::Workspace(w)) if w.id == chat.workspace.id => break w,
            Ok(HubNotice::Session(s)) => assert_eq!(s.status, SessionStatus::Running, "the turn ended first: {s:?}"),
            Ok(_) => {}
            Err(e) => panic!("the feed didn't say the workspace was worked in: {e}"),
        }
    };
    assert_eq!(told.user_active_at_ms, queued);
    chat.finishes(None, TurnOutcome::Success).await;
    chat.given(2).await;
    let listed = chat.hub.records_list().await.unwrap().workspaces;
    let listed = listed.into_iter().find(|w| w.id == chat.workspace.id).unwrap();
    assert_eq!(listed.user_active_at_ms, queued);
}

/// Its files are written into the worktree as it's sent, and the agent is given it where they are.
#[tokio::test]
async fn a_queued_prompt_carries_its_files_to_the_agent_and_a_change_keeps_them() {
    let dir = tempfile::tempdir().unwrap();
    let chat = Chat::at_work(dir.path()).await;
    let log = add(&chat, "a1", "build.log", MediaType::Text).await;

    let queued = chat.queue("q1", Prompt::carrying("before", vec![log.id.clone()])).await.unwrap();
    let changed = chat.change("q1", "attached").await.unwrap().queued;

    assert_eq!(queued.queued[0].prompt.files, std::slice::from_ref(&log));
    assert_eq!(changed[0].prompt.files, std::slice::from_ref(&log), "a change keeps them");
    chat.finishes(None, TurnOutcome::Success).await;
    let sent = chat.given(2).await[1];
    let recorded = chat.log().into_iter().find(|e| e.seq == sent).map(|e| e.body);
    let Some(EventBody::UserPrompt { prompt: Prompt { files, .. }, .. }) = recorded else { panic!("{recorded:?}") };
    assert_eq!(files, std::slice::from_ref(&log));
    let worktree = chat.workspace.path.clone().unwrap();
    let written = |request: &WorkerRequest| {
        let WorkerRequest::AttachmentWrite(write) = request else { return None };
        Some((write.path.clone(), write.name.clone(), write.data.as_slice().to_vec()))
    };
    assert_eq!(
        chat.asked.requests(written),
        [(worktree.clone(), "build.log".to_string(), b"error at line 42".to_vec())]
    );
    let given = |request: &WorkerRequest| {
        let WorkerRequest::AgentControl(asked) = request else { return None };
        let Control::Prompt { seq, prompt, .. } = &asked.control else { return None };
        (*seq == sent).then(|| prompt.clone())
    };
    let [given] = &chat.asked.requests(given)[..] else { panic!("given once") };
    assert_eq!(given.text, "attached");
    let placed: Vec<_> = given.files.iter().map(|file| (&file.attachment, file.path.as_str())).collect();
    assert_eq!(placed, [(&log, format!("{worktree}/.context/attachments/build.log").as_str())]);
}

#[tokio::test]
async fn a_queued_prompt_needs_text_or_files_and_files_the_session_has() {
    let dir = tempfile::tempdir().unwrap();
    // At work until the end, so nothing queued here is sent.
    let chat = Chat::at_work(dir.path()).await;
    let shot = add(&chat, "a1", "shot.png", MediaType::Png).await;

    let only_files = chat.queue("q1", Prompt::carrying("", vec![shot.id.clone()])).await.unwrap();
    let empty = chat.queue("q2", Prompt::new(" ")).await.map_err(|e| e.code);
    let unknown = chat.queue("q3", Prompt::carrying("look", vec!["a9".into()])).await.map_err(|e| e.code);

    assert_eq!(only_files.queued[0].prompt.files, [shot]);
    assert_eq!(empty.err(), Some(ErrorCode::InvalidRequest));
    assert_eq!(unknown.err(), Some(ErrorCode::NotFound));
    assert_eq!(chat.record().queued.len(), 1, "neither was queued");
}
