//! `gh`'s login on the hub's machine as a GitHub account, against a stand-in for `gh` in the test's `bin`, where the
//! test's hub finds it, which prints who it's logged in as and its token from files the test rewrites, and a stand-in
//! for GitHub.

use std::path::Path;

use protocol::client::ErrorCode;
use protocol::client::github_accounts::GithubAccountConnect;
use protocol::model::{GhLogin, GhLoginWorks, GithubLoginKind, GithubToken, SentCredential, UnusableLogin};
use protocol::worker::GithubCredential;

use crate::github_logins::GhSaid;
use crate::hub::Hub;
use crate::hub::tests::{hub_calling_github, hub_calling_github_again};
use crate::store::now_ms;
use github::{Answer, StandIn};

/// A stand-in for `gh` in `dir`'s `bin`, logged in as nobody until [`log_in`] says.
fn gh(dir: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let (status, token) = (dir.join("gh-status").display().to_string(), dir.join("gh-token").display().to_string());
    let script = format!(
        "#!/bin/sh\ncase \"$1 $2\" in\n'auth status') /bin/cat '{status}';;\n'auth token') /bin/cat '{token}' || exit 1;;\n*) exit 9;;\nesac\n"
    );
    std::fs::write(bin.join("gh"), script).unwrap();
    std::fs::set_permissions(bin.join("gh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    log_out(dir);
}

/// Has the stand-in say it's logged in as `name`, with `token`.
fn log_in(dir: &Path, name: &str, token: &str) {
    let status = format!(r#"{{"hosts":{{"github.com":[{{"active":true,"login":"{name}"}}]}}}}"#);
    std::fs::write(dir.join("gh-status"), status).unwrap();
    std::fs::write(dir.join("gh-token"), token).unwrap();
}

fn log_out(dir: &Path) {
    std::fs::write(dir.join("gh-status"), r#"{"hosts":{}}"#).unwrap();
    let _ = std::fs::remove_file(dir.join("gh-token"));
}

/// Why workers are blocked, as the hub's settings say it, or `None` while they're sent a login.
fn blocked_for(hub: &Hub) -> Option<UnusableLogin> {
    match hub.git_credential().borrow().recorded() {
        Some(SentCredential::Blocked { why, .. }) => Some(why),
        _ => None,
    }
}

#[tokio::test]
async fn who_gh_is_logged_in_as_is_said_with_whether_github_takes_its_token() {
    let github = StandIn::start(vec![("ada-token", Answer::user(7, "ada", None))]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());

    let none = hub.hub_gh_login().await.unwrap();
    gh(dir.path());
    let logged_out = hub.hub_gh_login().await.unwrap();
    log_in(dir.path(), "ada", "ada-token");
    let ada = hub.hub_gh_login().await.unwrap();
    log_in(dir.path(), "ada", "a-revoked-token");
    let refused = hub.hub_gh_login().await.unwrap();

    assert_eq!(none, GhLogin::NotInstalled);
    assert_eq!(logged_out, GhLogin::LoggedOut);
    assert_eq!(ada, GhLogin::LoggedIn { username: "ada".into(), works: GhLoginWorks::Yes });
    assert_eq!(refused, GhLogin::LoggedIn { username: "ada".into(), works: GhLoginWorks::Refused });
    assert_eq!(hub.records_list().await.unwrap().github_accounts, [], "asking connects nothing");
}

/// Connecting `gh`'s login while it has none that works is refused, saying what's wrong with `gh` on the hub's machine,
/// by what it is rather than as "this Mac" or "there", since the person may be asking from another device, and what to
/// run before trying again.
#[tokio::test]
async fn connecting_gh_with_no_login_that_works_says_what_to_run_on_the_hubs_machine() {
    let github = StandIn::start(vec![("ada-token", Answer::user(7, "ada", None))]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());

    let none = hub.github_account_connect_gh().await.unwrap_err();
    gh(dir.path());
    let logged_out = hub.github_account_connect_gh().await.unwrap_err();
    log_in(dir.path(), "ada", "a-revoked-token");
    let refused = hub.github_account_connect_gh().await.unwrap_err();

    let machine =
        if cfg!(target_os = "macos") { "the Mac Roundhouse runs on" } else { "the computer Roundhouse runs on" };
    assert_eq!(none.code, ErrorCode::Conflict);
    assert_eq!(
        none.message,
        format!("gh isn't installed on {machine}. Install gh, run gh auth login, then try again.")
    );
    assert_eq!(logged_out.code, ErrorCode::Conflict);
    assert_eq!(logged_out.message, format!("gh on {machine} is logged out. Run gh auth login, then try again."));
    assert_eq!(refused.code, ErrorCode::InvalidRequest);
    assert_eq!(
        refused.message,
        format!("GitHub refused the token of gh on {machine}. Run gh auth login, then try again.")
    );
}

/// The hub never adopts it by itself, however its logins are checked, so only connecting it makes an account.
#[tokio::test]
async fn gh_s_login_is_an_account_only_once_connected_and_its_token_is_kept_nowhere() {
    let github = StandIn::start(vec![("ada-token", Answer::user(7, "ada", None))]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    gh(dir.path());
    log_in(dir.path(), "ada", "ada-token");

    hub.check_github_logins(now_ms()).await;
    let before = hub.records_list().await.unwrap().github_accounts;
    let connected = hub.github_account_connect_gh().await.unwrap();
    let again = hub.github_account_connect_gh().await.unwrap();

    assert_eq!(before, [], "checking adopts nothing");
    assert_eq!(connected.held_login.map(|held| held.how), Some(GithubLoginKind::Gh));
    assert_eq!(again, connected, "the same login again is the same account");
    let GithubCredential::Account(credential) = hub.git_credential().borrow().for_workers() else { panic!() };
    assert_eq!((credential.username.as_str(), credential.token.as_str()), ("ada", "ada-token"));
    let files: Vec<_> = std::fs::read_dir(dir.path().join("github-logins")).unwrap().map(Result::unwrap).collect();
    let [file] = files.as_slice() else { panic!("{files:?}") };
    let kept = std::fs::read_to_string(file.path()).unwrap();
    assert!(kept.contains("gh") && !kept.contains("ada-token"), "{kept}");
}

#[tokio::test]
async fn the_default_follows_gh_logging_out_and_switching_and_coming_back() {
    let users = [("ada-token", Answer::user(7, "ada", None)), ("grace-token", Answer::user(8, "grace", None))];
    let github = StandIn::start(users.into()).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    gh(dir.path());
    log_in(dir.path(), "ada", "ada-token");
    hub.github_account_connect_gh().await.unwrap();

    log_out(dir.path());
    hub.read_gh().await;
    let logged_out = blocked_for(&hub);
    log_in(dir.path(), "grace", "grace-token");
    hub.read_gh().await;
    let switched = blocked_for(&hub);
    log_in(dir.path(), "ada", "ada-token");
    hub.read_gh().await;
    let back = blocked_for(&hub);

    assert_eq!(logged_out, Some(UnusableLogin::GhLoggedOut));
    assert_eq!(switched, Some(UnusableLogin::GhOtherUser));
    assert_eq!(back, None);
}

/// The token is kept only in memory, so a hub that starts again reads `gh` before it serves anyone or carries on with
/// the agents an earlier hub left, rather than have them blocked until its first check.
#[tokio::test]
async fn a_hub_started_again_reads_gh_before_any_worker_is_sent_the_default() {
    let github = StandIn::start(vec![("ada-token", Answer::user(7, "ada", None))]).await;
    let dir = tempfile::tempdir().unwrap();
    gh(dir.path());
    log_in(dir.path(), "ada", "ada-token");
    hub_calling_github(dir.path(), github.api()).github_account_connect_gh().await.unwrap();

    let again = hub_calling_github_again(dir.path(), github.api());
    let unread = blocked_for(&again);
    again.tidy_logins().await;

    assert_eq!(unread, Some(UnusableLogin::GhUnread), "nothing's read yet");
    let GithubCredential::Account(credential) = again.git_credential().borrow().for_workers() else { panic!() };
    assert_eq!(credential.token.as_str(), "ada-token");
}

/// A `gh` that doesn't answer leaves the last token for a few minutes, so one slow read doesn't block agents, and no
/// longer.
#[tokio::test]
async fn a_token_read_from_gh_works_only_for_a_few_minutes_without_another_read() {
    let github = StandIn::start(vec![("ada-token", Answer::user(7, "ada", None))]).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    gh(dir.path());
    log_in(dir.path(), "ada", "ada-token");
    hub.github_account_connect_gh().await.unwrap();
    let now = now_ms();
    let (minute, unread) = (60_000, || GhSaid::Unread { why: "gh took more than 15s".into() });

    hub.github_logins.gh_read(unread(), now + 4 * minute).unwrap();
    let still = blocked_for(&hub);
    hub.github_logins.gh_read(unread(), now + 6 * minute).unwrap();
    let after = blocked_for(&hub);

    assert_eq!(still, None);
    assert_eq!(after, Some(UnusableLogin::GhUnread));
}

/// As when the person connected `gh`'s login beside the account they signed in, then disconnected it: the account goes,
/// the default stays, and connecting `gh`'s login again makes a new account.
#[tokio::test]
async fn disconnecting_gh_s_login_removes_its_account_and_leaves_the_default_and_it_can_be_connected_again() {
    let users = [("ada-token", Answer::user(7, "ada", None)), ("signed-in-token", Answer::user(7, "ada", None))];
    let github = StandIn::start(users.to_vec()).await;
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_calling_github(dir.path(), github.api());
    gh(dir.path());
    log_in(dir.path(), "ada", "ada-token");
    let asking = |token: &str| GithubAccountConnect { token: GithubToken::new(token), id: None };
    let default = hub.github_account_connect(asking("signed-in-token")).await.unwrap();
    let from_gh = hub.github_account_connect_gh().await.unwrap();

    hub.github_account_disconnect(from_gh.id.clone()).await.unwrap();
    let after = hub.records_list().await.unwrap();
    let token_kept = hub.github_logins.gh_known(now_ms()).is_some();
    let again = hub.github_account_connect_gh().await.unwrap();

    assert_eq!(after.github_accounts, std::slice::from_ref(&default), "only gh's account went");
    assert_eq!(after.hub_settings.unwrap().github.unwrap().account, Some(default.id));
    assert!(!token_kept, "the token gh gave is forgotten with the last account that held it");
    assert_ne!(again.id, from_gh.id, "connecting it again makes a new account");
    assert_eq!(again.held_login.map(|held| held.how), Some(GithubLoginKind::Gh));
}
