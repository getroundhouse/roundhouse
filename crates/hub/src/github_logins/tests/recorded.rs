//! What the hub's settings say workers are sent for the default, as each change to the logins records it: what's sent,
//! without its token, moving the settings' revision only as that changes, so not as a renewal changes only the token.

use protocol::model::{SentCredential, UnusableLogin};

use super::credential::{reason, sent};
use super::renewals::renewed;
use super::sign_ins::{ada_signed_in, login_ending};
use super::*;

const HOUR: i64 = 60 * 60 * 1000;

/// When the sign-in's token ends, by the hub's clock.
const ENDS: i64 = 10 * HOUR;

/// What the settings say workers are sent, as the store has them.
fn recorded(store: &Store) -> Option<SentCredential> {
    store.hub_settings().unwrap().github.unwrap().sent
}

#[test]
fn the_settings_say_what_workers_are_sent_and_move_as_it_changes_but_not_as_a_renewal_changes_only_the_token() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let at_first = store.hub_settings().unwrap();
    let mut told = logins.recorded_settings();

    let account = ada_signed_in(&logins, &store, login_ending("a-token", ENDS));
    let signed_in = told.borrow_and_update().clone().unwrap();
    let due = logins.check(ENDS - 1).unwrap().due.remove(0);
    logins.renewed(&due, renewed(), ENDS - 1).unwrap();
    let renewal_told = told.has_changed().unwrap();
    let after_the_renewal = store.hub_settings().unwrap();
    // As after a Mac wakes past the renewed token's end, before the hub has renewed it again.
    logins.check(ENDS - 1 + 8 * HOUR).unwrap();
    let renewing = told.borrow_and_update().clone().unwrap();
    let renewing_reason = reason(sent(&logins));
    logins.set_default(None, ENDS + 8 * HOUR).unwrap();
    let machine_own = told.borrow_and_update().clone().unwrap();

    assert_eq!(at_first.github.unwrap().sent, Some(SentCredential::MachineOwn), "as the logins started");
    assert_eq!(signed_in.github.as_ref().unwrap().sent, Some(SentCredential::Account { account: account.clone() }));
    assert!(signed_in.rev > at_first.rev);
    assert!(!renewal_told, "a renewal changes only the token");
    assert_eq!(after_the_renewal, signed_in, "so the settings' revision stays");
    let blocked = SentCredential::Blocked { account, why: UnusableLogin::Renewing, reason: renewing_reason };
    assert_eq!(renewing.github.unwrap().sent, Some(blocked));
    assert!(renewing.rev > signed_in.rev);
    assert_eq!(machine_own.github.as_ref().unwrap().sent, Some(SentCredential::MachineOwn));
    assert_eq!(store.hub_settings().unwrap(), machine_own, "what's told is what the store has");
}

/// A token's end is on its account's record, but one only a newer build can read, or that's gone, isn't, and the
/// settings say which, so a client draws the hub's word rather than working it out. The settings the first account
/// makes the default are told of as they are once what's sent for it is recorded, never as they were just before.
#[test]
fn the_settings_say_why_a_default_blocks_as_workers_are_told() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let first = logins.connect(&connect("a-token", None, "g1"), &owner(7, "ada", Some(5000))).unwrap();
    let why = |store: &Store| match recorded(store) {
        Some(SentCredential::Blocked { why, reason, .. }) => Some((why, reason)),
        _ => None,
    };

    let before_its_end = recorded(&store);
    logins.check(5000).unwrap();
    let ended = why(&store).unwrap();
    logins.connect(&connect("another-token", Some("g1"), "g2"), &ada()).unwrap();
    let renewed = recorded(&store);
    logins.disconnect(&"g1".into(), 6000).unwrap();
    let disconnected = why(&store).unwrap();

    assert_eq!(before_its_end, Some(SentCredential::Account { account: "g1".into() }));
    assert_eq!(first.settings.and_then(|settings| settings.github).and_then(|github| github.sent), before_its_end);
    assert_eq!(ended.0, UnusableLogin::Ended);
    assert!(ended.1.contains("ada, has ended") && ended.1.contains("Renew it"), "{}", ended.1);
    assert_eq!(renewed, before_its_end, "connecting a token to it again has it sent again");
    assert_eq!(disconnected, (UnusableLogin::Disconnected, reason(sent(&logins))), "with the reason workers are");
}
