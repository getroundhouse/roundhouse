use std::path::Path;
use std::sync::Barrier;

use protocol::model::{LimitWindow, UsageLimit};

use super::*;
use crate::login_files::LoginFile;
use crate::store::tests::store_with_session;

mod handing;

/// The logins of a hub whose data directory is `dir`, and its store.
fn logins_in(dir: &Path) -> (Arc<Logins>, Arc<Store>) {
    let store = Arc::new(store_with_session(dir));
    (Arc::new(Logins::new(store.clone(), dir)), store)
}

/// A login of Claude's, named `name`, with `token`, connected at 1000 and ending at 2000. A new account gets an ID
/// made from `name`.
fn connect(name: &str, token: &str, account: Option<&AccountId>) -> Connect {
    Connect {
        agent: AgentKind::Claude,
        name: name.to_string(),
        token: LoginToken::new(token),
        account: account.cloned(),
        new_id: AccountId::from(format!("claude-login-{name}")),
        connected_at_ms: 1000,
        ends_at_ms: Some(2000),
    }
}

/// Every file in the logins directory, by what's in it.
fn tokens_kept(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir.join("logins")) else { return Vec::new() };
    let mut tokens: Vec<String> =
        entries.map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap()).collect();
    tokens.sort();
    tokens
}

/// Has the database refuse every write of a row's login, as `trigger` says when, as a full disk or a failed sync would.
fn refuse_writes_of_logins(dir: &Path, trigger: &str) {
    let conn = rusqlite::Connection::open(dir.join("hub.db")).unwrap();
    conn.execute_batch(&format!("CREATE TRIGGER refuse {trigger} BEGIN SELECT RAISE(ABORT, 'refused'); END;")).unwrap();
}

const ON_UPDATE: &str = "BEFORE UPDATE OF login_file ON accounts";

fn asked(name: &str, token: &str) -> AccountConnect {
    AccountConnect { agent: AgentKind::Claude, name: name.to_string(), token: LoginToken::new(token), account: None }
}

const LASTS_A_DAY: LoginDescription = LoginDescription { made_with: String::new(), lasts_ms: Some(86_400_000) };

#[test]
fn what_a_client_asks_is_trimmed_and_given_its_end_and_what_cant_be_connected_is_refused() {
    let connect = Connect::new(asked("  Work \n", " a-made-up-token\r\n"), &LASTS_A_DAY, 1000).unwrap();
    let long_name = "n".repeat(201);
    let long_token = "t".repeat(4097);

    let refused: Vec<_> = [
        asked(" ", "a-token"),
        asked(&long_name, "a-token"),
        asked("Work", " \n"),
        asked("Work", &long_token),
        asked("Work", "a-made-up\n-token"),
        asked("Work", "a made-up token"),
        asked("Work", "a-made-up\u{0}token"),
    ]
    .into_iter()
    .map(|asked| Connect::new(asked, &LASTS_A_DAY, 1000).err().map(|e| e.code))
    .collect();

    assert_eq!((connect.name.as_str(), connect.token.as_str()), ("Work", "a-made-up-token"));
    assert_eq!(connect.ends_at_ms, Some(1000 + 86_400_000));
    assert!(
        connect.new_id.as_str().strip_prefix("claude-login-").is_some_and(|rest| uuid::Uuid::try_parse(rest).is_ok())
    );
    assert!(refused.iter().all(|code| *code == Some(ErrorCode::InvalidRequest)), "{refused:?}");
}

#[test]
fn a_token_connected_is_kept_in_a_file_of_its_own_and_the_record_says_the_hub_holds_it() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());

    let AccountChange { account, changed } = logins.connect(&connect("Work", "a-made-up-token", None)).unwrap();

    assert!(changed);
    assert_eq!(account.id, AccountId::from("claude-login-Work"));
    assert_eq!((account.name.as_deref(), account.login_agent), (Some("Work"), Some(AgentKind::Claude)));
    let held = account.held_login.unwrap();
    assert_eq!((held.connected_at_ms, held.ends_at_ms), (1000, Some(2000)));
    assert_eq!(tokens_kept(dir.path()), ["a-made-up-token"]);
    assert_eq!(store.account(&account.id).unwrap(), Some(account), "the record a client is given is the one kept");
}

#[test]
fn the_same_token_again_finds_its_account_and_under_another_name_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let first = logins.connect(&connect("Work", "a-made-up-token", None)).unwrap().account;

    let again = logins.connect(&connect("Work", "a-made-up-token", None)).unwrap();
    let renamed = logins.connect(&Connect { connected_at_ms: 1500, ..connect("Day job", "a-made-up-token", None) });

    assert_eq!(again, AccountChange { account: first.clone(), changed: false });
    let refused = renamed.unwrap_err();
    assert_eq!(refused.code, ErrorCode::Conflict);
    assert!(refused.message.contains("Work"), "it says which account holds it: {}", refused.message);
    assert_eq!(store.account(&first.id).unwrap(), Some(first), "the account is as it was");
    assert_eq!(tokens_kept(dir.path()), ["a-made-up-token"]);
}

#[test]
fn renewing_replaces_the_login_and_the_name_and_keeps_what_was_heard_of_the_accounts_limits() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let id = logins.connect(&connect("Work", "an-old-token", None)).unwrap().account.id;
    let week = UsageLimit {
        window: LimitWindow::SevenDays,
        model: None,
        used_percent: 40,
        resets_at_ms: None,
        heard_at_ms: None,
    };
    store.hear_limits(&id, &[week], false, 1200).unwrap();

    let renewed =
        Connect { connected_at_ms: 3000, ends_at_ms: Some(4000), ..connect("Day job", "a-new-token", Some(&id)) };
    let account = logins.connect(&renewed).unwrap().account;

    assert_eq!((&account.id, account.name.as_deref()), (&id, Some("Day job")));
    assert_eq!(account.held_login.map(|held| (held.connected_at_ms, held.ends_at_ms)), Some((3000, Some(4000))));
    assert_eq!(account.limits.iter().map(|limit| limit.used_percent).collect::<Vec<_>>(), [40]);
    assert_eq!(tokens_kept(dir.path()), ["a-new-token"], "the old token is gone");
}

#[test]
fn renewing_with_the_token_the_account_holds_changes_nothing_but_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let first = logins.connect(&connect("Work", "a-made-up-token", None)).unwrap().account;

    let again = logins.connect(&connect("Work", "a-made-up-token", Some(&first.id))).unwrap();
    let renamed = logins.connect(&connect("Day job", "a-made-up-token", Some(&first.id))).unwrap().account;

    assert_eq!(again, AccountChange { account: first.clone(), changed: false });
    assert_eq!((renamed.name.as_deref(), renamed.held_login), (Some("Day job"), first.held_login));
}

#[test]
fn renewing_is_refused_for_no_account_an_adapters_account_and_a_token_another_holds_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let work = logins.connect(&connect("Work", "work-token", None)).unwrap().account;
    logins.connect(&connect("Home", "home-token", None)).unwrap();
    let adapters = AccountId::from("claude-0123456789abcdef");
    store.hear_limits(&adapters, &[], false, 0).unwrap();
    let records = || store.list_records().unwrap().accounts;
    let before = records();

    let nobody = logins.connect(&connect("Other", "new-token", Some(&"claude-login-nobody".into())));
    let adapter = logins.connect(&connect("Other", "new-token", Some(&adapters)));
    let taken = logins.connect(&connect("Other", "home-token", Some(&work.id)));

    assert_eq!(nobody.unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(adapter.unwrap_err().code, ErrorCode::InvalidRequest);
    assert_eq!(taken.unwrap_err().code, ErrorCode::Conflict);
    assert_eq!(records(), before);
    assert_eq!(tokens_kept(dir.path()), ["home-token", "work-token"]);
}

#[test]
fn disconnecting_forgets_the_token_and_keeps_the_record_and_again_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let id = logins.connect(&connect("Work", "a-made-up-token", None)).unwrap().account.id;

    let disconnected = logins.disconnect(&id).unwrap();
    let again = logins.disconnect(&id).unwrap();

    assert!(disconnected.changed);
    let account = &disconnected.account;
    assert_eq!(account.held_login, None);
    assert_eq!((account.name.as_deref(), account.login_agent), (Some("Work"), Some(AgentKind::Claude)));
    assert_eq!(again, AccountChange { account: disconnected.account.clone(), changed: false });
    assert_eq!(tokens_kept(dir.path()), Vec::<String>::new());
}

#[test]
fn disconnecting_an_account_with_no_record_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());

    let refused = logins.disconnect(&"claude-login-nobody".into());

    assert_eq!(refused.unwrap_err().code, ErrorCode::NotFound);
}

#[test]
fn after_a_disconnect_the_same_token_makes_a_new_account_unless_the_old_one_is_named() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let first = logins.connect(&connect("Work", "a-made-up-token", None)).unwrap().account.id;
    logins.disconnect(&first).unwrap();

    let unnamed = Connect { new_id: "claude-login-again".into(), ..connect("Work", "a-made-up-token", None) };
    let second = logins.connect(&unnamed).unwrap().account;
    logins.disconnect(&second.id).unwrap();
    let named = logins.connect(&connect("Work", "a-made-up-token", Some(&first))).unwrap().account;

    assert_eq!(second.id, AccountId::from("claude-login-again"));
    assert_eq!(named.id, first);
    assert!(named.held_login.is_some());
}

#[test]
fn a_renewal_whose_row_cant_be_written_keeps_the_old_login_whole() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let before = logins.connect(&connect("Work", "an-old-token", None)).unwrap().account;
    refuse_writes_of_logins(dir.path(), ON_UPDATE);

    let refused = logins.connect(&connect("Day job", "a-new-token", Some(&before.id)));

    assert_eq!(refused.unwrap_err().code, ErrorCode::Internal);
    assert_eq!(tokens_kept(dir.path()), ["an-old-token"]);
    assert_eq!(store.account(&before.id).unwrap(), Some(before));
}

#[test]
fn a_disconnect_whose_row_cant_be_cleared_has_already_forgotten_the_token_and_tidying_clears_it() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let id = logins.connect(&connect("Work", "a-made-up-token", None)).unwrap().account.id;
    refuse_writes_of_logins(dir.path(), ON_UPDATE);

    let refused = logins.disconnect(&id);
    let left = tokens_kept(dir.path());
    rusqlite::Connection::open(dir.path().join("hub.db")).unwrap().execute_batch("DROP TRIGGER refuse").unwrap();
    logins.tidy().unwrap();

    assert_eq!(refused.unwrap_err().code, ErrorCode::Internal);
    assert_eq!(left, Vec::<String>::new());
    assert_eq!(store.account(&id).unwrap().unwrap().held_login, None);
}

/// Connects `token` from eight threads at once, under the names `name` gives each, and answers with what each got.
/// Each would make an account of its own ID, as each request's would be, so only the lock keeps them to one.
fn eight_at_once(
    logins: &Arc<Logins>,
    token: &'static str,
    name: fn(usize) -> String,
) -> Vec<Result<AccountId, ErrorCode>> {
    let barrier = Arc::new(Barrier::new(8));
    let connects = Vec::from_fn(8, |at| {
        let (logins, barrier) = (logins.clone(), barrier.clone());
        std::thread::spawn(move || {
            barrier.wait();
            let new_id = AccountId::from(format!("claude-login-{token}-{at}"));
            let connect = Connect { new_id, ..connect(&name(at), token, None) };
            logins.connect(&connect).map(|change| change.account.id).map_err(|e| e.code)
        })
    });
    connects.into_iter().map(|connect| connect.join().unwrap()).collect()
}

#[test]
fn eight_connects_of_one_token_at_once_make_one_account() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());

    let mut alike = eight_at_once(&logins, "a-made-up-token", |_| "Work".to_string());
    let named_apart = eight_at_once(&logins, "another-made-up-token", |at| format!("Work {at}"));

    alike.dedup();
    assert!(matches!(alike[..], [Ok(_)]), "the same again finds the one account: {alike:?}");
    let (made, refused): (Vec<_>, Vec<_>) = named_apart.into_iter().partition(Result::is_ok);
    assert_eq!(made.len(), 1, "{made:?}");
    assert_eq!(refused, vec![Err(ErrorCode::Conflict); 7]);
    assert_eq!(tokens_kept(dir.path()), ["a-made-up-token", "another-made-up-token"]);
}

#[test]
fn tidying_clears_a_login_whose_file_isnt_a_token_and_removes_a_file_no_account_names() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let kept = logins.connect(&connect("Kept", "kept-token", None)).unwrap().account.id;
    let garbled = logins.connect(&connect("Garbled", "garbled-token", None)).unwrap().account.id;
    let entries = std::fs::read_dir(dir.path().join("logins")).unwrap();
    let mut paths = entries.map(|entry| entry.unwrap().path());
    let garbled_file = paths.find(|path| std::fs::read_to_string(path).unwrap() == "garbled-token").unwrap();
    std::fs::write(garbled_file, [0xff, 0xfe, 0xfd]).unwrap();
    std::fs::write(dir.path().join("logins").join(LoginFile::new().as_str()), "a-stray-token").unwrap();

    logins.tidy().unwrap();

    let held = |id: &AccountId| store.account(id).unwrap().unwrap().held_login.is_some();
    assert!(held(&kept) && !held(&garbled));
    assert_eq!(tokens_kept(dir.path()), ["kept-token"]);
}
