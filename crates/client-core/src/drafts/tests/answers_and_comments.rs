//! Keeping what's typed in answer to a chat's question, and the comment being written on a workspace's diff, each apart
//! from the chat's draft.

use protocol::model::{AnswerSoFar, DiffSide};

use super::*;

fn answer(typed: &str) -> AnswerDraft {
    let so_far = AnswerSoFar { picked: Vec::new(), typed: typed.to_string(), other: true };
    AnswerDraft { approval_id: "q".into(), answers: vec![so_far] }
}

fn comment(text: &str) -> CommentDraft {
    CommentDraft { path: "src/lib.rs".into(), side: DiffSide::New, line: 12, editing: None, text: text.to_string() }
}

fn workspace(id: &str) -> WorkspaceId {
    id.into()
}

/// In the order of the places, since the keeper keeps different places in no particular order.
fn sorted(mut said: Vec<(String, String)>) -> Vec<(String, String)> {
    said.sort();
    said
}

#[test]
fn an_answer_and_a_comment_are_each_kept_apart_from_the_chats_draft() {
    let dir = tempfile::tempdir().unwrap();
    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    let drafts = Drafts::start(&client);
    drafts.set(DraftChange::Composer(chat("a"), DraftToKeep::new("first")));
    kept(&drafts, &heard);

    drafts.typed(DraftChange::Composer(chat("a"), DraftToKeep::new("typed")));
    drafts.typed(DraftChange::Answer(chat("a"), Some(answer("half"))));
    drafts.typed(DraftChange::Answer(chat("a"), Some(answer("half an answer"))));
    drafts.typed(DraftChange::Comment(workspace("w"), Some(comment("why?"))));

    assert!(drafts.unkept(&DraftKey::Answer(chat("a"))));
    assert!(drafts.unkept(&DraftKey::Comment(workspace("w"))));
    let said = pairs(&[("a", "answer half an answer"), ("a", "typed"), ("w", "comment why?")]);
    assert_eq!(sorted(kept(&drafts, &heard)), said, "only the newest of each");
    let places = [DraftKey::Composer(chat("a")), DraftKey::Answer(chat("a")), DraftKey::Comment(workspace("w"))];
    assert!(places.iter().all(|at| !drafts.unkept(at)));
}

#[test]
fn what_a_hub_that_keeps_only_chats_drafts_refuses_waits_for_the_next_connection_and_the_rest_goes_on() {
    let dir = tempfile::tempdir().unwrap();
    let (older, heard) = hub(&dir.path().join("older.sock"), Answer::KeepsOnlyChatsDrafts);
    let drafts = Drafts::start(&older);
    drafts.set(DraftChange::Answer(chat("a"), Some(answer("half"))));
    drafts.set(DraftChange::Comment(workspace("w"), Some(comment("why?"))));
    let refused = kept(&drafts, &heard);
    assert_eq!(refused, pairs(&[("a", "refused answer half"), ("w", "refused comment why?")]));

    drafts.set(DraftChange::Composer(chat("a"), DraftToKeep::new("typed")));

    assert_eq!(kept(&drafts, &heard), pairs(&[("a", "typed")]), "the rest goes on, and the hub isn't asked again");
    assert!(drafts.unkept(&DraftKey::Answer(chat("a"))), "it would be lost if the app stopped now");
    assert!(drafts.unkept(&DraftKey::Comment(workspace("w"))));
    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    drafts.reconnect(&client);
    assert_eq!(sorted(kept(&drafts, &heard)), pairs(&[("a", "answer half"), ("w", "comment why?")]));
    assert!(!drafts.unkept(&DraftKey::Answer(chat("a"))));
}

/// What's held for the next connection is kept all at once on it, so a hub that refuses one kind of place is asked
/// about it once, rather than for each place of that kind.
#[test]
fn a_hub_that_refuses_a_kind_of_place_is_asked_about_it_once_a_connection() {
    let dir = tempfile::tempdir().unwrap();
    let (gone, _) = hub(&dir.path().join("gone.sock"), Answer::HangsUp);
    let drafts = Drafts::start(&gone);
    for (chat_id, workspace_id) in [("a", "w"), ("b", "x")] {
        drafts.set(DraftChange::Answer(chat(chat_id), Some(answer("half"))));
        drafts.set(DraftChange::Comment(workspace(workspace_id), Some(comment("why?"))));
    }
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();
    let (older, heard) = hub(&dir.path().join("older.sock"), Answer::KeepsOnlyChatsDrafts);

    drafts.reconnect(&older);

    let asked = kept(&drafts, &heard);
    let refused = |kind: &str| asked.iter().filter(|(_, what)| what.starts_with(&format!("refused {kind}"))).count();
    assert_eq!((refused("answer"), refused("comment")), (1, 1), "{asked:?}");
}

/// The chat's record at `rev`, with `typed` in answer to its question.
fn answered(rev: u64, typed: &str) -> Session {
    Session { answer_draft: Some(answer(typed)), ..told("a", rev, "") }
}

#[test]
fn each_place_tells_what_was_made_elsewhere_by_its_own_copy() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::start(&keeping_hub(&dir.path().join("hub.sock"), 5));

    drafts.set(DraftChange::Answer(chat("a"), Some(answer("half"))));
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();
    drafts.set(DraftChange::Comment(workspace("w"), Some(comment("why?"))));
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();

    assert!(
        !drafts.from_elsewhere(&KeptDraft::answer(&answered(5, "half"))),
        "the record the hub kept it in is this client's"
    );
    assert!(!drafts.from_elsewhere(&KeptDraft::answer(&answered(7, "half"))), "and one newer for something else");
    assert!(drafts.from_elsewhere(&KeptDraft::answer(&answered(8, "half an answer"))));
    let typed_elsewhere = Session { answer_draft: Some(answer("half")), ..told("a", 5, "typed elsewhere") };
    assert!(
        drafts.from_elsewhere(&KeptDraft::composer(&typed_elsewhere)),
        "nothing was typed in the chat's composer here"
    );
    assert!(!drafts.from_elsewhere(&KeptDraft::comment(&writing("w", 6, Some(comment("why?"))))));
    assert!(drafts.from_elsewhere(&KeptDraft::comment(&writing("w", 7, Some(comment("why not?"))))));
}

#[test]
fn an_answer_or_a_comment_taken_from_a_record_isnt_from_elsewhere_until_another_is_made_there() {
    let dir = tempfile::tempdir().unwrap();
    let (client, _) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    let drafts = Drafts::start(&client);

    drafts.took(KeptDraft::answer(&answered(3, "half")));
    drafts.took(KeptDraft::comment(&writing("w", 3, Some(comment("why?")))));

    assert!(!drafts.from_elsewhere(&KeptDraft::answer(&answered(4, "half"))));
    assert!(drafts.from_elsewhere(&KeptDraft::answer(&answered(5, "half an answer"))));
    assert!(!drafts.from_elsewhere(&KeptDraft::comment(&writing("w", 4, Some(comment("why?"))))));
    assert!(drafts.from_elsewhere(&KeptDraft::comment(&writing("w", 5, None))));
}
