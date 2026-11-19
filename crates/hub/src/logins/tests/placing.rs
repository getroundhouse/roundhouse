//! Which account Automatic hands a launch of a chat on none yet, of the logins the hub holds, and how choosing an
//! account ends it.

use super::super::*;
use super::{connect, logins_in};
use crate::store::{Placed, SessionLogin};
use protocol::client::ApiError;
use protocol::model::{HubSettings, Reserve};

/// The account a launch of a chat on none yet is handed at `now`, and what put it there.
fn placed(logins: &Logins, now: i64) -> Result<(AccountId, Placed), ApiError> {
    let handed = logins.to_hand(AgentKind::Claude, &SessionLogin::NoneYet, now)?.unwrap();
    Ok((handed.account, handed.placed))
}

fn automatic(settings: &HubSettings) -> bool {
    settings.account_defaults.iter().flatten().any(|choice| choice.agent == AgentKind::Claude && choice.automatic)
}

#[test]
fn automatic_puts_a_chat_on_the_account_with_usage_to_spare_until_an_account_is_chosen() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let no_login = logins.set_automatic(AgentKind::Claude).unwrap_err();
    let work = logins.connect(&connect("Work", "work-token", None)).unwrap().account.id;
    let personal = logins.connect(&connect("Personal", "personal-token", None)).unwrap().account.id;
    logins.set_reserve(&work, Some(Reserve::ALL)).unwrap();

    let set = logins.set_automatic(AgentKind::Claude).unwrap();
    let again = logins.set_automatic(AgentKind::Claude).unwrap();
    let on_personal = placed(&logins, 1700).unwrap();
    logins.set_reserve(&personal, Some(Reserve::ALL)).unwrap();
    let none_to_spare = placed(&logins, 1700).unwrap_err();
    let chosen = logins.set_default(AgentKind::Claude, &work, 1700).unwrap();

    assert_eq!(no_login.code, ErrorCode::Conflict);
    assert!(automatic(&set));
    assert_eq!(again, set, "the same again changes nothing");
    assert_eq!(on_personal, (personal, Placed::Automatically), "all of Work is reserved");
    assert_eq!(none_to_spare.code, ErrorCode::Conflict);
    assert!(none_to_spare.message.contains("no account has usage to spare"), "{}", none_to_spare.message);
    assert!(!automatic(&chosen) && chosen.rev > set.rev, "choosing an account ends Automatic");
    assert_eq!(placed(&logins, 1700).unwrap(), (work, Placed::ByDefault), "a default ignores what's reserved");
}

/// With no added login that works, Automatic says to renew or reconnect one, rather than that none has usage to spare,
/// since waiting for usage wouldn't help.
#[test]
fn automatic_with_no_login_that_works_says_to_renew_one() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let work = logins.connect(&connect("Work", "work-token", None)).unwrap().account.id;
    logins.set_automatic(AgentKind::Claude).unwrap();
    logins.disconnect(&work).unwrap();

    let refused = placed(&logins, 1700).unwrap_err();

    assert_eq!(refused.code, ErrorCode::Conflict);
    assert!(refused.message.contains("renew or reconnect one"), "{}", refused.message);
}
