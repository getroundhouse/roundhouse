//! A chat's draft is kept by when it was typed, so a change the hub refuses, since one made elsewhere was typed after
//! it, leaves the hub's draft in the composer in its place, as `SessionDraft` says.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::stand_in_hub;
use crate::stand_in_hub::Does;
use protocol::client::{ClientRequest, ClientResponse, ReplyOutcome, drafts};

/// Now, by the system's clock, as a change is stamped.
fn now_ms() -> i64 {
    i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()).unwrap()
}

/// A hub that keeps each chat's draft, at revision 5, and says when each change it's sent says it was typed.
fn stamping_hub(socket: &Path) -> (Connection, flume::Receiver<Option<i64>>) {
    let (said, heard) = flume::unbounded();
    let client = stand_in_hub::connect(socket, move |request| {
        let ClientRequest::SessionDraft(drafts::SessionDraft { id, draft, typed_at_ms }) = request else {
            return Does::Nothing;
        };
        let _ = said.send(typed_at_ms);
        Does::Answers(Box::new(ReplyOutcome::Response(ClientResponse::Session(told(id.as_str(), 5, &draft.text)))))
    });
    (client, heard)
}

/// It keeps when it was typed while it waits, such as on a phone that can't reach the hub, rather than taking the
/// time it's sent.
#[test]
fn a_change_says_it_was_typed_as_it_was_handed_over_rather_than_as_it_was_sent() {
    let dir = tempfile::tempdir().unwrap();
    let (gone, _) = hub(&dir.path().join("gone.sock"), Answer::HangsUp);
    let drafts = Drafts::start(&gone);
    let before = now_ms();
    drafts.typed(DraftChange::Composer(chat("a"), DraftToKeep::new("typed with no hub")));
    let typed = now_ms();
    drafts.flushed().recv_timeout(TIMEOUT).unwrap();
    waiting::thread::until(TIMEOUT, "the clock to pass when it was typed", || now_ms() > typed);
    let (client, stamps) = stamping_hub(&dir.path().join("hub.sock"));

    drafts.reconnect(&client);

    let stamp = stamps.recv_timeout(TIMEOUT).unwrap().expect("a chat's draft says when it was typed");
    assert!((before..=typed).contains(&stamp), "typed between {before} and {typed}, and it says {stamp}");
}

/// The case that comparing with the copy agreed on misses: the hub's draft is the one this client last had, since what
/// was typed elsewhere after this said the same, so only the refusal says to show it.
#[test]
fn a_change_refused_for_one_typed_after_it_leaves_the_hubs_draft_though_this_client_had_it_already() {
    let dir = tempfile::tempdir().unwrap();
    let (client, _) = hub(&dir.path().join("hub.sock"), Answer::TypedLaterElsewhere);
    let drafts = Drafts::start(&client);
    let hubs = told("a", 3, "typed before");
    assert_eq!(drafts.take_composer(&hubs), "typed before");

    drafts.typed(DraftChange::Composer(chat("a"), DraftToKeep::new("typed on the train")));

    drafts.caught_up().recv_timeout(TIMEOUT).unwrap();
    assert!(!drafts.unkept(&DraftKey::Composer(chat("a"))), "the hub answered, so nothing's on its way");
    assert!(drafts.from_elsewhere(&KeptDraft::composer(&hubs)), "the hub's is shown, though it's what was agreed");
    assert_eq!(drafts.take_composer(&hubs), "typed before");
    assert!(!drafts.from_elsewhere(&KeptDraft::composer(&hubs)), "and once it's taken, it's this client's again");
}

#[test]
fn a_change_from_here_kept_after_one_refused_is_this_clients_again() {
    let dir = tempfile::tempdir().unwrap();
    let (refusing, _) = hub(&dir.path().join("refusing.sock"), Answer::TypedLaterElsewhere);
    let drafts = Drafts::start(&refusing);
    drafts.typed(DraftChange::Composer(chat("a"), DraftToKeep::new("typed on the train")));
    drafts.caught_up().recv_timeout(TIMEOUT).unwrap();
    drafts.reconnect(&keeping_hub(&dir.path().join("hub.sock"), 5));

    drafts.typed(DraftChange::Composer(chat("a"), DraftToKeep::new("typed after")));

    drafts.caught_up().recv_timeout(TIMEOUT).unwrap();
    assert!(!drafts.from_elsewhere(&KeptDraft::composer(&told("a", 5, "typed after"))), "the record it was kept in");
    assert!(!drafts.from_elsewhere(&KeptDraft::composer(&told("a", 4, "typed elsewhere"))), "nor an older one");
}
