//! Connecting a token, disconnecting it, choosing the default and tidying, as GitHub accounts' logins.

use std::path::Path;
use std::sync::Barrier;

use protocol::model::GithubLoginKind;

use super::*;
use crate::login_files::LoginFile;
use crate::store::tests::store_with_session;

/// The GitHub logins of a hub whose data directory is `dir`, started at 1000, and its store.
pub(super) fn logins_in(dir: &Path) -> (Arc<GithubLogins>, Arc<Store>) {
    let store = Arc::new(store_with_session(dir));
    (Arc::new(GithubLogins::new(store.clone(), dir, 1000)), store)
}

/// `token`, connected at 1000, renewing `id`'s login, or else making the account `new_id`.
pub(super) fn connect(token: &str, id: Option<&str>, new_id: &str) -> Connect {
    Connect {
        token: GithubToken::new(token),
        id: id.map(GithubAccountId::from),
        new_id: new_id.into(),
        connected_at_ms: 1000,
    }
}

/// The GitHub user `name`, of the number `user`, whose token ends at `ends_at_ms`.
pub(super) fn owner(user: u64, name: &str, ends_at_ms: Option<i64>) -> TokenOwner {
    TokenOwner { user: GithubUser { id: GithubUserId::new(user), name: name.into() }, ends_at_ms }
}

pub(super) fn ada() -> TokenOwner {
    owner(7, "ada", None)
}

/// Every file in the GitHub logins' directory, by what's in it.
fn tokens_kept(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir.join("github-logins")) else { return Vec::new() };
    let mut tokens: Vec<String> =
        entries.map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap()).collect();
    tokens.sort();
    tokens
}

pub(super) fn default_of(store: &Store) -> Option<GithubAccountId> {
    store.hub_settings().unwrap().github.unwrap().account
}

#[test]
fn what_a_client_asks_is_trimmed_and_given_an_id_of_its_own_and_a_token_that_cant_be_one_is_refused() {
    let asked = |token: &str| GithubAccountConnect { token: GithubToken::new(token), id: None };

    let connect = Connect::new(asked(" github_pat_made_up\r\n"), 1000).unwrap();
    let refused =
        ["", "a made up token", "made\nup"].map(|token| Connect::new(asked(token), 1000).err().map(|e| e.code));

    assert_eq!(connect.token.as_str(), "github_pat_made_up");
    assert!(uuid::Uuid::try_parse(connect.new_id.as_str()).is_ok(), "{}", connect.new_id);
    assert_eq!(refused, [Some(ErrorCode::InvalidRequest); 3]);
}

#[test]
fn a_token_is_kept_in_a_file_of_its_own_and_only_the_first_account_becomes_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());

    let first = logins.connect(&connect("first-token", None, "g1"), &owner(7, "ada", Some(5000))).unwrap();
    store.set_github_default(None).unwrap();
    let second = logins.connect(&connect("second-token", None, "g2"), &owner(8, "grace", None)).unwrap();

    assert!(first.changed);
    assert_eq!(first.account.user, owner(7, "ada", None).user);
    let held = first.account.held_login.unwrap();
    assert_eq!((held.how, held.connected_at_ms, held.ends_at_ms), (GithubLoginKind::Token, 1000, Some(5000)));
    assert_eq!(first.settings.and_then(|settings| settings.github).unwrap().account, Some("g1".into()));
    assert_eq!(second.settings, None, "the second isn't the default, though none is");
    assert_eq!(default_of(&store), None);
    assert_eq!(tokens_kept(dir.path()), ["first-token", "second-token"]);
    assert!(!dir.path().join("logins").exists(), "agents' logins have a directory of their own");
    assert_eq!(store.list_records().unwrap().github_accounts, [first.account, second.account]);
}

#[test]
fn the_same_token_again_is_its_account_changing_nothing_before_github_is_asked_and_after() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    let first = logins.connect(&connect("a-token", None, "g1"), &ada()).unwrap().account;

    let unasked = logins.held(&connect("a-token", None, "g2")).unwrap();
    let renewing_its_own = logins.held(&connect("a-token", Some("g1"), "g2")).unwrap();
    let asked = logins.connect(&connect("a-token", None, "g2"), &ada()).unwrap();
    let another = logins.held(&connect("another-token", None, "g2")).unwrap();

    let unchanged = GithubAccountChange { account: first, changed: false, settings: None };
    assert_eq!(unasked.as_ref(), Some(&unchanged));
    assert_eq!(renewing_its_own.as_ref(), Some(&unchanged));
    assert_eq!(asked, unchanged);
    assert_eq!(another, None, "GitHub has to say whose it is");
    assert_eq!(tokens_kept(dir.path()), ["a-token"]);
}

#[test]
fn renewing_replaces_the_token_in_place_takes_the_users_new_name_and_keeps_the_default() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    logins.connect(&connect("an-old-token", None, "g1"), &ada()).unwrap();

    let renewed = Connect { connected_at_ms: 3000, ..connect("a-new-token", Some("g1"), "g2") };
    let change = logins.connect(&renewed, &owner(7, "ada-renamed", Some(9000))).unwrap();

    assert_eq!((change.account.id.as_str(), change.account.user.name.as_str()), ("g1", "ada-renamed"));
    assert_eq!(change.account.held_login.map(|held| (held.connected_at_ms, held.ends_at_ms)), Some((3000, Some(9000))));
    assert_eq!(change.settings, None);
    assert_eq!(default_of(&store), Some("g1".into()));
    assert_eq!(tokens_kept(dir.path()), ["a-new-token"], "the old token is gone");
}

#[test]
fn renewing_is_refused_for_another_users_token_one_another_account_holds_and_no_account_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    logins.connect(&connect("work-token", None, "work"), &ada()).unwrap();
    logins.connect(&connect("home-token", None, "home"), &owner(8, "grace", None)).unwrap();
    let before = store.list_records().unwrap().github_accounts;

    let held_by_home = logins.held(&connect("home-token", Some("work"), "g"));
    let graces = logins.connect(&connect("new-token", Some("work"), "g"), &owner(8, "grace", None));
    let nobody = logins.held(&connect("new-token", Some("nobody"), "g"));

    assert_eq!(held_by_home.unwrap_err().code, ErrorCode::Conflict);
    assert_eq!(graces.unwrap_err().code, ErrorCode::Conflict);
    assert_eq!(nobody.unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(store.list_records().unwrap().github_accounts, before);
    assert_eq!(tokens_kept(dir.path()), ["home-token", "work-token"]);
}

#[test]
fn disconnecting_forgets_the_token_and_keeps_the_record_and_the_default_and_again_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    logins.connect(&connect("a-token", None, "g1"), &ada()).unwrap();

    let disconnected = logins.disconnect(&"g1".into(), 2000).unwrap();
    let again = logins.disconnect(&"g1".into(), 2000).unwrap();
    let nobody = logins.disconnect(&"nobody".into(), 2000);
    let connected_again = logins.held(&connect("a-token", None, "g2")).unwrap();

    assert!(disconnected.changed);
    assert_eq!((disconnected.account.held_login, &disconnected.account.user), (None, &ada().user));
    assert_eq!(again, GithubAccountChange { account: disconnected.account, changed: false, settings: None });
    assert_eq!(nobody.unwrap_err().code, ErrorCode::NotFound);
    assert_eq!(default_of(&store), Some("g1".into()), "a lost default stays the default");
    assert_eq!(tokens_kept(dir.path()), Vec::<String>::new());
    assert_eq!(connected_again, None, "the hub keeps nothing to know the token by");
}

#[test]
fn the_default_is_refused_for_an_account_with_no_record_or_no_login_that_can_be_used_and_can_be_the_machines_own() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    logins.connect(&connect("work-token", None, "work"), &ada()).unwrap();
    logins.connect(&connect("old-token", None, "ended"), &owner(7, "ada", Some(1500))).unwrap();
    logins.connect(&connect("gone-token", None, "gone"), &ada()).unwrap();
    logins.disconnect(&"gone".into(), 2000).unwrap();
    logins.connect(&connect("lost-token", None, "lost"), &ada()).unwrap();
    let mut paths = std::fs::read_dir(dir.path().join("github-logins")).unwrap().map(|entry| entry.unwrap().path());
    std::fs::remove_file(paths.find(|path| std::fs::read_to_string(path).unwrap() == "lost-token").unwrap()).unwrap();

    let refused =
        ["nobody", "ended", "gone", "lost"].map(|id| logins.set_default(Some(&id.into()), 2000).unwrap_err().code);
    let after_refusals = default_of(&store);
    let machines_own = logins.set_default(None, 2000).unwrap();

    assert_eq!(refused, [ErrorCode::NotFound, ErrorCode::Conflict, ErrorCode::Conflict, ErrorCode::Conflict]);
    assert_eq!(after_refusals, Some("work".into()), "each refusal left the default as it was");
    assert_eq!(machines_own.github.unwrap().account, None);
}

#[test]
fn choosing_the_default_moves_the_settings_revision_and_choosing_it_again_doesnt() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connect(&connect("work-token", None, "work"), &ada()).unwrap();
    logins.connect(&connect("home-token", None, "home"), &ada()).unwrap();

    let home = logins.set_default(Some(&"home".into()), 2000).unwrap();
    let again = logins.set_default(Some(&"home".into()), 2000).unwrap();

    assert_eq!(home.github.as_ref().unwrap().account, Some("home".into()));
    assert_eq!(again, home);
}

#[test]
fn tidying_clears_a_login_whose_file_isnt_a_token_and_removes_what_no_account_names_but_not_agents_logins() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    logins.connect(&connect("kept-token", None, "kept"), &ada()).unwrap();
    logins.connect(&connect("garbled-token", None, "garbled"), &ada()).unwrap();
    let github_logins = dir.path().join("github-logins");
    let mut paths = std::fs::read_dir(&github_logins).unwrap().map(|entry| entry.unwrap().path());
    let garbled = paths.find(|path| std::fs::read_to_string(path).unwrap() == "garbled-token").unwrap();
    std::fs::write(garbled, [0xff, 0xfe, 0xfd]).unwrap();
    std::fs::write(github_logins.join(LoginFile::new().as_str()), "a-stray-token").unwrap();
    std::fs::create_dir(dir.path().join("logins")).unwrap();
    std::fs::write(dir.path().join("logins").join(LoginFile::new().as_str()), "an-agents-token").unwrap();

    logins.tidy(2000).unwrap();

    let held = |id: &str| store.github_account(&id.into()).unwrap().unwrap().held_login.is_some();
    assert!(held("kept") && !held("garbled"));
    assert_eq!(tokens_kept(dir.path()), ["kept-token"]);
    assert_eq!(std::fs::read_dir(dir.path().join("logins")).unwrap().count(), 1, "an agent's login is another's");
}

/// Each would make an account of its own ID, as each request's would be, so only checking again under the lock keeps
/// them to one.
#[test]
fn eight_connects_of_one_token_at_once_make_one_account() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let barrier = Arc::new(Barrier::new(8));

    let connects = Vec::from_fn(8, |at| {
        let (logins, barrier) = (logins.clone(), barrier.clone());
        std::thread::spawn(move || {
            barrier.wait();
            logins.connect(&connect("a-token", None, &format!("g{at}")), &ada()).unwrap().account.id
        })
    });
    let mut made: Vec<GithubAccountId> = connects.into_iter().map(|connect| connect.join().unwrap()).collect();

    made.dedup();
    assert_eq!(made.len(), 1, "{made:?}");
    assert_eq!(store.list_records().unwrap().github_accounts.len(), 1);
    assert_eq!(tokens_kept(dir.path()), ["a-token"]);
}
