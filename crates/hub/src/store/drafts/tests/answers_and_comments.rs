//! What's typed in answer to a chat's question, and in a comment on a workspace's diff.

use protocol::event::EventBody;
use protocol::model::{AnswerSoFar, CommentDraft, DecisionDraft, DiffSide, SoFar, WorkspaceId};

use super::id;
use crate::store::tests::{open_in, store_with_session};
use crate::store::{StoreError, cancelled_approval};

/// Answers to the request `q`'s two questions, with `typed` for the first, being written.
fn answer(typed: &str) -> DecisionDraft {
    let so_far = AnswerSoFar { picked: vec![1], typed: typed.to_string(), other: true };
    DecisionDraft { approval_id: "q".into(), so_far: SoFar::Answers { answers: vec![so_far, AnswerSoFar::default()] } }
}

fn comment(text: &str) -> CommentDraft {
    CommentDraft { path: "src/lib.rs".into(), side: DiffSide::New, line: 12, editing: None, text: text.to_string() }
}

#[test]
fn a_decision_being_drafted_is_kept_until_another_replaces_it_and_none_clears_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    assert_eq!(store.get_session(&id()).unwrap().session.decision_draft, None, "nothing's typed yet");
    let before = store.get_session(&id()).unwrap().session.rev;

    let kept = store.set_decision_draft(&id(), Some(&answer("half"))).unwrap();

    assert!(kept.changed);
    assert!(
        kept.session.session.rev > before,
        "clients follow records by revision, so the feed would miss it otherwise"
    );
    assert_eq!(kept.session.session, store.get_session(&id()).unwrap().session, "it comes back as stored");
    assert_eq!(kept.session.session.decision_draft, Some(answer("half")));
    store.set_decision_draft(&id(), Some(&answer("half an answer"))).unwrap();
    let again = store.set_decision_draft(&id(), Some(&answer("half an answer"))).unwrap();
    assert!(!again.changed, "nothing changed, so nothing is announced");
    assert_eq!(again.session.session.decision_draft, Some(answer("half an answer")));
    let cleared = [None, None].map(|draft| store.set_decision_draft(&id(), draft).unwrap().changed);
    assert_eq!(cleared, [true, false]);
    assert_eq!(store.get_session(&id()).unwrap().session.decision_draft, None);
}

#[test]
fn closing_a_chat_lets_go_of_the_decision_being_drafted_and_it_keeps_none_after() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.set_decision_draft(&id(), Some(&answer("half"))).unwrap();

    store.set_session_closed(&id(), Some(1)).unwrap();

    assert_eq!(store.get_session(&id()).unwrap().session.decision_draft, None);
    let typed_late = store.set_decision_draft(&id(), Some(&answer("typed as it closed"))).unwrap();
    assert!(!typed_late.changed, "a closed session keeps none");
}

#[test]
fn deciding_a_request_takes_the_draft_of_it_and_none_is_kept_for_it_after() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.set_decision_draft(&id(), Some(&answer("half"))).unwrap();
    store.append(&id(), None, &cancelled_approval("another".into()), None, None, None).unwrap();
    let decision_draft = || store.get_session(&id()).unwrap().session.decision_draft;
    assert_eq!(decision_draft(), Some(answer("half")), "another question's answer isn't this one's");

    store.append(&id(), None, &cancelled_approval("q".into()), None, None, None).unwrap();

    assert_eq!(decision_draft(), None);
    let typed_late = store.set_decision_draft(&id(), Some(&answer("typed elsewhere as it was answered"))).unwrap();
    assert!(!typed_late.changed, "nobody can send it now");
    let next = DecisionDraft { approval_id: "next".into(), ..answer("for the next question") };
    assert!(store.set_decision_draft(&id(), Some(&next)).unwrap().changed);
}

#[test]
fn a_draft_with_nothing_chosen_or_typed_is_kept_as_none() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    store.set_decision_draft(&id(), Some(&answer("half"))).unwrap();
    let other = AnswerSoFar { other: true, ..AnswerSoFar::default() };
    let nothing = DecisionDraft {
        approval_id: "q".into(),
        so_far: SoFar::Answers { answers: vec![other, AnswerSoFar::default()] },
    };

    let kept = store.set_decision_draft(&id(), Some(&nothing)).unwrap();

    assert!(kept.changed);
    assert_eq!(kept.session.session.decision_draft, None);
    assert!(!store.set_decision_draft(&id(), None).unwrap().changed, "it's the same as none");
}

#[test]
fn a_comment_being_written_is_kept_on_its_workspace_until_none_clears_it() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let workspace = WorkspaceId::from("w");
    let before = store.get_workspace(&workspace).unwrap();
    assert_eq!(before.comment_draft, None, "nothing's being written yet");

    let (kept, changed) = store.set_comment_draft(&workspace, Some(&comment("half"))).unwrap();

    assert!(changed);
    assert!(kept.rev > before.rev, "clients follow records by revision, so the feed would miss it otherwise");
    assert_eq!(kept, store.get_workspace(&workspace).unwrap(), "it comes back as stored");
    assert_eq!(kept.comment_draft, Some(comment("half")));
    let editing = CommentDraft { editing: Some("c".into()), ..comment("changed") };
    store.set_comment_draft(&workspace, Some(&editing)).unwrap();
    let (again, changed) = store.set_comment_draft(&workspace, Some(&editing)).unwrap();
    assert!(!changed, "nothing changed, so nothing is announced");
    assert_eq!(again.comment_draft, Some(editing));
    let cleared = [None, None].map(|draft| store.set_comment_draft(&workspace, draft).unwrap().1);
    assert_eq!(cleared, [true, false]);
    assert_eq!(store.get_workspace(&workspace).unwrap().comment_draft, None);
}

#[test]
fn a_comment_being_written_on_a_workspace_that_isnt_there_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store = open_in(dir.path());

    let refused = store.set_comment_draft(&"w".into(), Some(&comment("half")));

    assert!(matches!(refused, Err(StoreError::NotFound { kind: "workspace", .. })), "{refused:?}");
}

#[test]
fn secret_answers_never_reach_the_shared_draft_or_recorded_decision() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let request: EventBody = serde_json::from_value(serde_json::json!({
        "type":"approval_requested","approval_id":"q",
        "request":{"asks":{"asks":"questions","questions":[
            {"text":"Secret?","typed":"secret"},{"text":"Public?","typed":"yes"}
        ]}}
    }))
    .unwrap();
    store.append(&id(), None, &request, None, None, None).unwrap();
    let secret = AnswerSoFar { typed: "synthetic-secret".into(), other: true, picked: Vec::new() };
    let public = AnswerSoFar { typed: "public".into(), other: true, picked: Vec::new() };
    let draft =
        DecisionDraft { approval_id: "q".into(), so_far: SoFar::Answers { answers: vec![secret, public.clone()] } };
    store.set_decision_draft(&id(), Some(&draft)).unwrap();
    let kept = store.get_session(&id()).unwrap().session.decision_draft.unwrap();
    assert_eq!(kept.so_far, SoFar::Answers { answers: vec![AnswerSoFar::default(), public] });
}
