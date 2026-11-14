//! Which login the hub gives a launch, of those it holds.

use super::*;

fn handed(logins: &Logins, now: i64) -> Option<(AccountId, String)> {
    logins
        .to_hand(AgentKind::Claude, now)
        .unwrap()
        .map(|handed| (handed.account, handed.login.token.as_str().to_string()))
}

#[test]
fn the_first_account_is_default_and_only_an_explicit_choice_changes_it() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    assert!(handed(&logins, 1000).is_none());
    let first = logins.connect(&connect("Work", "work-token", None)).unwrap().account.id;
    let other = logins.connect(&connect("Personal", "personal-token", None)).unwrap().account.id;
    assert_eq!(handed(&logins, 1700), Some((first.clone(), "work-token".into())));
    let before = store.hub_settings().unwrap();
    let changed = logins.set_default(AgentKind::Claude, &other, 1700).unwrap();
    assert!(changed.rev > before.rev);
    assert_eq!(logins.set_default(AgentKind::Claude, &other, 1700).unwrap(), changed);
    logins.connect(&connect("Work", "renewed-token", Some(&first))).unwrap();
    assert_eq!(handed(&logins, 1700), Some((other.clone(), "personal-token".into())));
    drop(logins);
    drop(store);
    let store = Store::open(&dir.path().join("hub.db"), &dir.path().join("backups")).unwrap();
    let reopened = Logins::new(Arc::new(store), dir.path().join("logins"));
    assert_eq!(handed(&reopened, 1700), Some((other, "personal-token".into())));
}

#[test]
fn expiry_disconnect_and_missing_tokens_block_instead_of_changing_identity() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let first = logins.connect(&connect("Work", "work-token", None)).unwrap().account.id;
    logins.connect(&Connect { ends_at_ms: None, ..connect("Personal", "personal-token", None) }).unwrap();
    assert_eq!(logins.to_hand(AgentKind::Claude, 2000).unwrap_err().code, ErrorCode::Conflict);
    logins.disconnect(&first).unwrap();
    assert_eq!(logins.to_hand(AgentKind::Claude, 1700).unwrap_err().code, ErrorCode::Conflict);
    logins.connect(&connect("Work", "renewed-token", Some(&first))).unwrap();
    let files = std::fs::read_dir(dir.path().join("logins")).unwrap();
    let path = files
        .map(|file| file.unwrap().path())
        .find(|path| std::fs::read_to_string(path).unwrap() == "renewed-token")
        .unwrap();
    std::fs::remove_file(path).unwrap();
    logins.tidy().unwrap();
    assert_eq!(logins.to_hand(AgentKind::Claude, 1700).unwrap_err().code, ErrorCode::Conflict);
    assert!(logins.to_hand(AgentKind::Unknown, 1700).unwrap().is_none());
}

#[test]
fn selecting_requires_a_live_held_login_of_the_same_kind() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let id = logins.connect(&connect("Work", "work-token", None)).unwrap().account.id;
    assert_eq!(logins.set_default(AgentKind::Unknown, &id, 1000).unwrap_err().code, ErrorCode::InvalidRequest);
    assert_eq!(logins.set_default(AgentKind::Claude, &id, 2000).unwrap_err().code, ErrorCode::Conflict);
    logins.disconnect(&id).unwrap();
    assert_eq!(logins.set_default(AgentKind::Claude, &id, 1000).unwrap_err().code, ErrorCode::Conflict);
}
