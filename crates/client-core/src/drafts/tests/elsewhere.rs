//! Telling a chat's draft made somewhere else, such as in another client, from one kept from here.

use super::*;

#[test]
fn a_newer_record_is_from_elsewhere_only_when_its_draft_isnt_the_one_kept_from_here() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::start(&keeping_hub(&dir.path().join("hub.sock"), 5));

    drafts.set(DraftChange::Composer(chat("a"), DraftToKeep::new("typed here")));
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();

    assert!(
        !drafts.from_elsewhere(&KeptDraft::composer(&told("a", 5, "typed here"))),
        "the record the hub kept it in is this client's"
    );
    assert!(
        !drafts.from_elsewhere(&KeptDraft::composer(&told("a", 4, "typed before"))),
        "and so is an older one that arrives late"
    );
    assert!(
        !drafts.from_elsewhere(&KeptDraft::composer(&told("a", 6, "typed here"))),
        "and one newer for something else, such as its status"
    );
    assert!(drafts.from_elsewhere(&KeptDraft::composer(&told("a", 7, "typed elsewhere"))));
    assert!(
        drafts.from_elsewhere(&KeptDraft::composer(&told("b", 1, "typed elsewhere"))),
        "nothing was typed there here"
    );
    assert!(!drafts.from_elsewhere(&KeptDraft::composer(&told("c", 1, ""))), "nor anything elsewhere");
}

#[test]
fn a_draft_taken_from_a_record_isnt_from_elsewhere_until_another_is_made_there() {
    let dir = tempfile::tempdir().unwrap();
    let (client, _) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    let drafts = Drafts::start(&client);
    let elsewhere = told("a", 3, "typed elsewhere");
    assert!(drafts.from_elsewhere(&KeptDraft::composer(&elsewhere)));

    drafts.took(KeptDraft::composer(&elsewhere));

    assert!(!drafts.from_elsewhere(&KeptDraft::composer(&elsewhere)));
    assert!(
        !drafts.from_elsewhere(&KeptDraft::composer(&told("a", 4, "typed elsewhere"))),
        "a newer record that carries it"
    );
    assert!(drafts.from_elsewhere(&KeptDraft::composer(&told("a", 5, "typed elsewhere again"))));
}

#[test]
fn nothing_from_elsewhere_is_taken_while_a_draft_from_here_is_on_its_way() {
    let dir = tempfile::tempdir().unwrap();
    let (gone, _) = hub(&dir.path().join("gone.sock"), Answer::HangsUp);
    let drafts = Drafts::start(&gone);
    drafts.typed(DraftChange::Composer(chat("a"), DraftToKeep::new("typed as the hub went")));
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();

    assert!(
        !drafts.from_elsewhere(&KeptDraft::composer(&told("a", 100, "typed elsewhere"))),
        "what's typed here goes on and replaces it"
    );
    assert!(drafts.caught_up().try_recv().is_err());
    drafts.reconnect(&keeping_hub(&dir.path().join("hub.sock"), 5));

    drafts.caught_up().recv_timeout(TIMEOUT).unwrap();
    assert!(!drafts.unkept(&DraftKey::Composer(chat("a"))));
    assert!(
        drafts.from_elsewhere(&KeptDraft::composer(&told("a", 100, "typed elsewhere"))),
        "a newer record, once nothing is on its way"
    );
    assert!(!drafts.from_elsewhere(&KeptDraft::composer(&told("a", 5, "typed as the hub went"))));
}
