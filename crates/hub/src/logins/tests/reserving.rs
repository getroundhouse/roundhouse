use super::super::*;
use super::{connect, logins_in};
use protocol::model::Reserve;

#[test]
fn a_reserve_is_on_the_record_through_a_disconnect_and_the_same_again_or_none_again_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let id = logins.connect(&connect("Work", "a-made-up-token", None)).unwrap().account.id;
    let quarter = Reserve::new(25);

    let set = logins.set_reserve(&id, quarter).unwrap();
    let again = logins.set_reserve(&id, quarter).unwrap();
    logins.disconnect(&id).unwrap();
    let kept = store.account(&id).unwrap().unwrap().reserve;
    let cleared = logins.set_reserve(&id, None).unwrap();
    let cleared_again = logins.set_reserve(&id, None).unwrap();

    assert!(set.changed);
    assert_eq!(set.account.reserve, quarter);
    assert_eq!(again, AccountChange { account: set.account.clone(), changed: false, defaults: None });
    assert_eq!(kept, quarter, "a disconnect keeps the record, and what it reserves");
    assert!(cleared.changed);
    assert!(cleared.account.rev > set.account.rev);
    assert_eq!(cleared.account.reserve, None);
    assert!(!cleared_again.changed);
}

#[test]
fn reserving_is_refused_for_no_account_and_an_adapters_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let adapters = AccountId::from("claude-0123456789abcdef");
    store.hear_limits(&adapters, &[], false, 0).unwrap();
    let before = store.list_records().unwrap().accounts;

    let nobody = logins.set_reserve(&"claude-login-nobody".into(), Reserve::new(10));
    let adapter = logins.set_reserve(&adapters, Reserve::new(10));

    assert_eq!(nobody.unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(adapter.unwrap_err().code, ErrorCode::InvalidRequest);
    assert_eq!(store.list_records().unwrap().accounts, before);
}
