//! Which login the hub gives a launch, of those it holds.

use super::*;
use crate::store::SessionLogin;

fn handed(logins: &Logins, now: i64) -> Option<(AccountId, String)> {
    logins
        .to_hand(AgentKind::Claude, &SessionLogin::NoneYet, now)
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
    let reopened = Logins::new(Arc::new(store), dir.path());
    assert_eq!(handed(&reopened, 1700), Some((other, "personal-token".into())));
}

#[test]
fn expiry_disconnect_and_missing_tokens_block_instead_of_changing_identity() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let first = logins.connect(&connect("Work", "work-token", None)).unwrap().account.id;
    logins.connect(&Connect { ends_at_ms: None, ..connect("Personal", "personal-token", None) }).unwrap();
    assert_eq!(logins.to_hand(AgentKind::Claude, &SessionLogin::NoneYet, 2000).unwrap_err().code, ErrorCode::Conflict);
    logins.disconnect(&first).unwrap();
    assert_eq!(logins.to_hand(AgentKind::Claude, &SessionLogin::NoneYet, 1700).unwrap_err().code, ErrorCode::Conflict);
    logins.connect(&connect("Work", "renewed-token", Some(&first))).unwrap();
    let files = std::fs::read_dir(dir.path().join("logins")).unwrap();
    let path = files
        .map(|file| file.unwrap().path())
        .find(|path| std::fs::read_to_string(path).unwrap() == "renewed-token")
        .unwrap();
    std::fs::remove_file(path).unwrap();
    logins.tidy().unwrap();
    assert_eq!(logins.to_hand(AgentKind::Claude, &SessionLogin::NoneYet, 1700).unwrap_err().code, ErrorCode::Conflict);
    assert!(logins.to_hand(AgentKind::Unknown, &SessionLogin::NoneYet, 1700).unwrap().is_none());
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

/// The account a chat is on is handed, whichever is the default, and doesn't change when it does. A chat on the
/// ambient login is handed none, whatever the default is.
#[test]
fn a_chats_account_is_handed_whatever_the_default_is_and_a_chat_on_the_ambient_login_none() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let work = logins.connect(&connect("Work", "work-token", None)).unwrap().account.id;
    let personal = logins.connect(&connect("Personal", "personal-token", None)).unwrap().account.id;
    let on = |account: &AccountId| {
        logins.to_hand(AgentKind::Claude, &SessionLogin::Hub(account.clone()), 1700).unwrap().unwrap().account
    };

    assert_eq!(on(&personal), personal);
    logins.set_default(AgentKind::Claude, &personal, 1700).unwrap();
    assert_eq!(on(&work), work);
    assert_eq!(handed(&logins, 1700).unwrap().0, personal, "a chat on none yet is handed the default");
    assert!(logins.to_hand(AgentKind::Claude, &SessionLogin::Ambient, 1700).unwrap().is_none());
}

/// A chat whose account can't be used is blocked, as a new one whose default can't be is, rather than falling back to
/// the default, and the refusal says it's the chat's account.
#[test]
fn a_chats_account_that_cant_be_used_blocks_it_rather_than_falling_back() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connect(&connect("Work", "work-token", None)).unwrap();
    let personal = logins.connect(&connect("Personal", "personal-token", None)).unwrap().account.id;

    logins.disconnect(&personal).unwrap();

    let refused = logins.to_hand(AgentKind::Claude, &SessionLogin::Hub(personal), 1700).unwrap_err();
    assert_eq!(refused.code, ErrorCode::Conflict);
    assert!(refused.message.contains("this chat's account Personal"), "{}", refused.message);
    assert!(handed(&logins, 1700).is_some(), "the default is still handed to chats on none yet");
}

/// A move is refused where choosing the account as the default would be, since it would leave the chat unable to
/// launch, and the refusal names the account alone, since it isn't the chat's yet.
#[test]
fn a_move_is_refused_as_choosing_a_default_is() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connect(&connect("Work", "work-token", None)).unwrap();
    let personal = logins.connect(&connect("Personal", "personal-token", None)).unwrap().account.id;
    let move_to = |account: &AccountId, now| logins.can_move_to(AgentKind::Claude, account, now);

    assert!(move_to(&personal, 1000).is_ok());
    assert_eq!(move_to(&"nobody".into(), 1000).unwrap_err().code, ErrorCode::NotFound);
    let other_kind = logins.can_move_to(AgentKind::Unknown, &personal, 1000).unwrap_err();
    assert_eq!(other_kind.code, ErrorCode::InvalidRequest);
    let ended = move_to(&personal, 2000).unwrap_err();
    assert_eq!((ended.code, ended.message.as_str()), (ErrorCode::Conflict, PERSONAL_FIRST), "its login has ended");
    let as_default = logins.set_default(AgentKind::Claude, &personal, 2000).unwrap_err();
    assert_eq!(as_default.message, PERSONAL_FIRST, "nor is it the default yet");
    logins.disconnect(&personal).unwrap();
    assert_eq!(move_to(&personal, 1000).unwrap_err().code, ErrorCode::Conflict, "its login is gone");
}

const PERSONAL_FIRST: &str = "Personal needs a login; renew or reconnect it in Agents first";
