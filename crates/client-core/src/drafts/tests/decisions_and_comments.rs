//! Keeping what's decided so far on the request a chat's agent waits on, and the comment being written on a workspace's
//! diff, each apart from the chat's draft.

use protocol::model::{DiffSide, SoFar};

use super::*;

/// Denying the request `q` in the words `typed`, being written.
fn denying(typed: &str) -> DecisionDraft {
    DecisionDraft {
        approval_id: "q".into(),
        so_far: SoFar::Use { chosen: None, words: typed.to_string(), writing: true },
    }
}

fn comment(text: &str) -> CommentDraft {
    CommentDraft { path: "src/lib.rs".into(), side: DiffSide::New, line: 12, editing: None, text: text.to_string() }
}

fn workspace(id: &str) -> WorkspaceId {
    id.into()
}

/// In the order of the drafts, since the keeper keeps different drafts in no particular order.
fn sorted(mut said: Vec<(String, String)>) -> Vec<(String, String)> {
    said.sort();
    said
}

#[test]
fn a_decision_and_a_comment_are_each_kept_apart_from_the_chats_draft() {
    let dir = tempfile::tempdir().unwrap();
    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    let drafts = Drafts::start(&client);

    drafts.typed(DraftChange::Composer(chat("a"), DraftToKeep::new("typed")));
    drafts.typed(DraftChange::Decision(chat("a"), Some(denying("half"))));
    drafts.typed(DraftChange::Decision(chat("a"), Some(denying("half an answer"))));
    drafts.typed(DraftChange::Comment(workspace("w"), Some(comment("why?"))));

    assert!(drafts.unkept(&DraftKey::Decision(chat("a"))));
    assert!(drafts.unkept(&DraftKey::Comment(workspace("w"))));
    let said = pairs(&[("a", "decision half an answer"), ("a", "typed"), ("w", "comment why?")]);
    assert_eq!(sorted(kept(&drafts, &heard)), said, "only the newest of each");
    let keys = [DraftKey::Composer(chat("a")), DraftKey::Decision(chat("a")), DraftKey::Comment(workspace("w"))];
    assert!(keys.iter().all(|key| !drafts.unkept(key)));
}

/// The chat's record at `rev`, with its request denied in the words `typed`, being written.
fn drafted(rev: u64, typed: &str) -> Session {
    Session { decision_draft: Some(denying(typed)), ..told("a", rev, "") }
}

#[test]
fn each_draft_tells_what_was_made_elsewhere_by_its_own_copy() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::start(&keeping_hub(&dir.path().join("hub.sock"), 5));

    drafts.set(DraftChange::Decision(chat("a"), Some(denying("half"))));
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();
    drafts.set(DraftChange::Comment(workspace("w"), Some(comment("why?"))));
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();

    assert!(
        !drafts.from_elsewhere(&KeptDraft::decision(&drafted(5, "half"))),
        "the record the hub kept it in is this client's"
    );
    assert!(!drafts.from_elsewhere(&KeptDraft::decision(&drafted(7, "half"))), "and one newer for something else");
    assert!(drafts.from_elsewhere(&KeptDraft::decision(&drafted(8, "half an answer"))));
    let typed_elsewhere = Session { decision_draft: Some(denying("half")), ..told("a", 5, "typed elsewhere") };
    assert!(
        drafts.from_elsewhere(&KeptDraft::composer(&typed_elsewhere)),
        "nothing was typed in the chat's composer here"
    );
    assert!(!drafts.from_elsewhere(&KeptDraft::comment(&writing("w", 6, Some(comment("why?"))))));
    assert!(drafts.from_elsewhere(&KeptDraft::comment(&writing("w", 7, Some(comment("why not?"))))));
}

#[test]
fn a_decision_or_a_comment_taken_from_a_record_isnt_from_elsewhere_until_another_is_made_there() {
    let dir = tempfile::tempdir().unwrap();
    let (client, _) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    let drafts = Drafts::start(&client);

    drafts.took(KeptDraft::decision(&drafted(3, "half")));
    drafts.took(KeptDraft::comment(&writing("w", 3, Some(comment("why?")))));

    assert!(!drafts.from_elsewhere(&KeptDraft::decision(&drafted(4, "half"))));
    assert!(drafts.from_elsewhere(&KeptDraft::decision(&drafted(5, "half an answer"))));
    assert!(!drafts.from_elsewhere(&KeptDraft::comment(&writing("w", 4, Some(comment("why?"))))));
    assert!(drafts.from_elsewhere(&KeptDraft::comment(&writing("w", 5, None))));
}
