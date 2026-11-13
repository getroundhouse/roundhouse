//! Keeping what's in the new-workspace dialog, with its files, apart from every chat's draft.

use super::*;

fn dialog(text: &str, files: &[&str]) -> NewWorkspaceDraftToKeep {
    let files = files.iter().map(|&id| id.into()).collect();
    NewWorkspaceDraftToKeep { text: text.to_string(), files, ..NewWorkspaceDraftToKeep::default() }
}

/// The dialog's record as the hub's feed might tell of it, at `rev`, with `typed` in it.
fn told(rev: u64, typed: &str) -> NewWorkspaceDraft {
    NewWorkspaceDraft { rev: Rev::new(rev), text: typed.to_string(), ..NewWorkspaceDraft::default() }
}

#[test]
fn the_dialogs_files_go_before_the_draft_that_names_them_and_are_let_go_of_after_the_one_that_stops() {
    let dir = tempfile::tempdir().unwrap();
    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    let drafts = Drafts::start(&client);

    drafts.attach(FilesFor::NewWorkspace, file("f1"));
    drafts.typed(DraftChange::NewWorkspace(dialog("fix the login", &["f1"])));
    drafts.set(DraftChange::Composer(chat("a"), DraftToKeep::new("a chat's")));

    assert!(drafts.unkept(&DraftKey::NewWorkspace));
    let said = kept(&drafts, &heard);
    let dialogs: Vec<_> = said.iter().filter(|(at, _)| at == "dialog").cloned().collect();
    assert_eq!(dialogs, pairs(&[("dialog", "+f1"), ("dialog", "fix the login f1")]));
    assert!(said.contains(&("a".to_string(), "a chat's".to_string())), "a chat's draft is kept apart");
    assert!(!drafts.unkept(&DraftKey::NewWorkspace));

    drafts.typed(DraftChange::NewWorkspace(dialog("", &[])));
    drafts.detach(FilesFor::NewWorkspace, &"f1".into());

    assert_eq!(kept(&drafts, &heard), pairs(&[("dialog", ""), ("dialog", "-f1")]));
}

#[test]
fn what_a_hub_that_keeps_only_chats_drafts_refuses_of_the_dialog_waits_for_the_next_connection_files_and_all() {
    let dir = tempfile::tempdir().unwrap();
    let (older, heard) = hub(&dir.path().join("older.sock"), Answer::KeepsOnlyChatsDrafts);
    let drafts = Drafts::start(&older);
    drafts.attach(FilesFor::NewWorkspace, file("f1"));
    drafts.set(DraftChange::NewWorkspace(dialog("fix the login", &["f1"])));
    drafts.attach(FilesFor::NewWorkspace, file("f2"));
    let refused = pairs(&[("dialog", "refused +f1"), ("dialog", "refused +f1")]);
    assert_eq!(kept(&drafts, &heard), refused, "its first file, as it is, then as one of a kind any hub knows");

    drafts.set(DraftChange::Composer(chat("a"), DraftToKeep::new("typed")));

    assert_eq!(kept(&drafts, &heard), pairs(&[("a", "typed")]), "the rest goes on, and the hub isn't asked again");
    assert!(drafts.unkept(&DraftKey::NewWorkspace), "it would be lost if the app stopped now");
    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    drafts.reconnect(&client);
    let said = pairs(&[("dialog", "+f1"), ("dialog", "+f2"), ("dialog", "fix the login f1")]);
    assert_eq!(kept(&drafts, &heard), said, "its files in the order they came");
    assert!(!drafts.unkept(&DraftKey::NewWorkspace));
}

/// As one the app knows and its hub doesn't, since the hub refuses that as it would a request it doesn't know.
#[test]
fn a_file_of_a_kind_the_hub_doesnt_know_goes_to_the_dialog_as_one_that_goes_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::KnowsNoKindOfFile);
    let drafts = Drafts::start(&client);

    drafts.attach(FilesFor::NewWorkspace, file("f1"));
    drafts.set(DraftChange::NewWorkspace(dialog("fix the login", &["f1"])));

    let said = pairs(&[("dialog", "refused +f1"), ("dialog", "+f1"), ("dialog", "fix the login f1")]);
    assert_eq!(kept(&drafts, &heard), said, "the dialog goes on");
    assert!(!drafts.unkept(&DraftKey::NewWorkspace));
}

#[test]
fn a_file_the_dialog_let_go_of_waits_for_a_hub_that_keeps_the_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let (older, heard) = hub(&dir.path().join("older.sock"), Answer::KeepsOnlyChatsDrafts);
    let drafts = Drafts::start(&older);

    drafts.detach(FilesFor::NewWorkspace, &"f1".into());

    assert_eq!(kept(&drafts, &heard), pairs(&[("dialog", "refused -f1")]));
    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    drafts.reconnect(&client);
    assert_eq!(kept(&drafts, &heard), pairs(&[("dialog", "-f1")]), "or that hub would keep it for good");
}

#[test]
fn a_file_let_go_of_before_it_reached_the_hub_never_goes_to_it() {
    let dir = tempfile::tempdir().unwrap();
    let (older, heard) = hub(&dir.path().join("older.sock"), Answer::KeepsOnlyChatsDrafts);
    let drafts = Drafts::start(&older);
    drafts.attach(FilesFor::NewWorkspace, file("f1"));
    drafts.set(DraftChange::NewWorkspace(dialog("look", &["f1"])));
    kept(&drafts, &heard);

    drafts.set(DraftChange::NewWorkspace(dialog("", &[])));
    drafts.detach(FilesFor::NewWorkspace, &"f1".into());

    let (client, heard) = hub(&dir.path().join("hub.sock"), Answer::Answered);
    drafts.reconnect(&client);
    assert_eq!(kept(&drafts, &heard), pairs(&[("dialog", "")]), "neither added nor let go of");
    assert!(!drafts.unkept(&DraftKey::NewWorkspace));
}

#[test]
fn the_dialog_tells_what_was_made_elsewhere_by_its_own_copy() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::start(&keeping_hub(&dir.path().join("hub.sock"), 5));
    assert!(
        drafts.from_elsewhere(&KeptDraft::new_workspace(&told(2, "typed elsewhere"))),
        "nothing was typed in it here"
    );
    assert!(!drafts.from_elsewhere(&KeptDraft::new_workspace(&told(2, ""))), "nor anything elsewhere");

    drafts.set(DraftChange::NewWorkspace(dialog("fix", &[])));
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();

    assert!(
        !drafts.from_elsewhere(&KeptDraft::new_workspace(&told(5, "fix"))),
        "the record the hub kept it in is this client's"
    );
    assert!(!drafts.from_elsewhere(&KeptDraft::new_workspace(&told(6, "fix"))), "and one newer that carries it");
    assert!(drafts.from_elsewhere(&KeptDraft::new_workspace(&told(7, "fix the login"))));
    assert!(
        drafts.from_elsewhere(&KeptDraft::composer(&super::told("a", 5, "typed elsewhere"))),
        "a chat's draft has its own"
    );
    drafts.took(KeptDraft::new_workspace(&told(7, "fix the login")));
    assert!(!drafts.from_elsewhere(&KeptDraft::new_workspace(&told(7, "fix the login"))), "once it's taken");
}

/// Each hub counts its revisions from its own, and the dialog is the same draft on every hub.
#[test]
fn another_connection_forgets_the_dialogs_copy_agreed_on() {
    let dir = tempfile::tempdir().unwrap();
    let drafts = Drafts::start(&keeping_hub(&dir.path().join("hub.sock"), 5));
    drafts.set(DraftChange::NewWorkspace(dialog("fix", &[])));
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();
    assert!(
        !drafts.from_elsewhere(&KeptDraft::new_workspace(&told(3, "typed on another hub"))),
        "older than the one kept"
    );

    drafts.reconnect(&keeping_hub(&dir.path().join("another.sock"), 1));
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();

    assert!(drafts.from_elsewhere(&KeptDraft::new_workspace(&told(3, "typed on another hub"))));
}
