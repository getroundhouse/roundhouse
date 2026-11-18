//! Which sign-ins a check finds due, and what renewing one comes to, as GitHub answered, under the logins' lock. When,
//! by the wall clock, is tested with the hub's check, in `crate::hub`.

use std::time::Duration;

use super::credential::{as_ada, reason, sent};
use super::sign_ins::{ada_signed_in, login_ending, sign_in_kept};
use super::*;
use crate::login_files::{LoginFile, Stored};
use github::{GithubRefreshToken, Granted, Renewal};

const HOUR: i64 = 60 * 60 * 1000;

/// When the token of each sign-in here ends, by the hub's clock, which a check within the hour before finds due.
const ENDS: i64 = 10 * HOUR;

/// Each renewal a check at `now_ms` finds due, by account and what renews its token.
fn due_at(logins: &GithubLogins, now_ms: i64) -> Vec<(String, String)> {
    let due = logins.check(now_ms).unwrap().due;
    due.iter().map(|due| (due.account.to_string(), due.refresh.as_str().to_string())).collect()
}

/// The one renewal a check at `now_ms` finds due.
fn only_due(logins: &GithubLogins, now_ms: i64) -> Due {
    let mut due = logins.check(now_ms).unwrap().due;
    assert_eq!(due.len(), 1, "{due:?}");
    due.remove(0)
}

/// What GitHub gives as it renews a token: `a-renewed-token`, lasting 8 hours, and `a-renewed-refresh`, about 181 days.
pub(super) fn renewed() -> Renewal {
    let refresh = (GithubRefreshToken::new("a-renewed-refresh"), Some(Duration::from_hours(181 * 24)));
    let lasts = Some(Duration::from_hours(8));
    Renewal::Renewed(Granted { token: GithubToken::new("a-renewed-token"), lasts, refresh: Some(refresh) })
}

fn refused() -> Renewal {
    Renewal::Refused("GitHub said bad_refresh_token: Made up.".to_string())
}

/// The file of the login the account `id` holds, and what it holds, as the logins read it.
fn held(logins: &GithubLogins, store: &Store, id: &str) -> (LoginFile, Stored<GithubLogin>) {
    let file = store.github_login_file(&id.into()).unwrap().unwrap();
    let held = logins.files.lock().read(&file).unwrap();
    (file, held)
}

/// A token connected to an account, which nothing renews, a sign-in whose token doesn't end, and one that needs
/// signing in again, beside one that's due.
#[test]
fn only_a_sign_in_whose_token_ends_within_the_hour_and_that_needs_no_signing_in_again_is_due() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    ada_signed_in(&logins, &store, login_ending("a-token", ENDS));
    logins.connect(&connect("a-pasted-token", None, "pasted"), &owner(8, "grace", Some(ENDS))).unwrap();
    let lasting = SignedIn { token_ends_at_ms: None, ..login_ending("a-lasting-token", ENDS) };
    sign_in_kept(&logins, &store, &owner(9, "lin", None), "lasting", lasting);
    sign_in_kept(&logins, &store, &owner(10, "mo", None), "refused", login_ending("a-refused-token", ENDS));
    store.github_login_needs_sign_in(&"refused".into()).unwrap();

    let due = due_at(&logins, ENDS - 1);

    assert_eq!(due, [("g".to_string(), "a-token-refresh".to_string())]);
}

/// One whose refresh token has ended by the check's clock, and one GitHub gave none.
#[test]
fn a_due_sign_in_that_nothing_can_renew_needs_signing_in_again_and_a_default_it_is_blocks_saying_so() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let ended = SignedIn { refresh_ends_at_ms: Some(ENDS - 2 * HOUR), ..login_ending("a-token", ENDS) };
    ada_signed_in(&logins, &store, ended);
    let none = SignedIn { refresh: None, ..login_ending("graces-token", ENDS) };
    sign_in_kept(&logins, &store, &owner(8, "grace", None), "none", none);

    let checked = logins.check(ENDS - 1).unwrap();
    let again = logins.check(ENDS - 1).unwrap();

    assert!(checked.due.is_empty(), "{checked:?}");
    let needing: Vec<_> = checked
        .changed
        .iter()
        .map(|changed| (changed.id.as_str(), changed.held_login.unwrap().needs_sign_in))
        .collect();
    assert_eq!(needing, [("g", true), ("none", true)]);
    let reason = reason(sent(&logins));
    assert!(reason.contains("ada, has ended") && reason.contains("Sign in with GitHub again"), "{reason}");
    assert!(again.changed.is_empty() && again.due.is_empty(), "the next check changes nothing: {again:?}");
}

#[test]
fn a_renewal_replaces_the_sign_ins_file_in_place_reaches_every_worker_and_moves_when_its_record_says_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    ada_signed_in(&logins, &store, login_ending("a-token", ENDS));
    let due = only_due(&logins, ENDS - 1);
    let (file, _) = held(&logins, &store, "g");
    let mut following = logins.git_credential();

    let change = logins.renewed(&due, renewed(), ENDS - 1).unwrap().unwrap();

    let renewed_ends = ENDS - 1 + 8 * HOUR;
    let kept = SignedIn {
        token: GithubToken::new("a-renewed-token"),
        token_ends_at_ms: Some(renewed_ends),
        refresh: Some(GithubRefreshToken::new("a-renewed-refresh")),
        refresh_ends_at_ms: Some(ENDS - 1 + 181 * 24 * HOUR),
    };
    assert_eq!(held(&logins, &store, "g"), (file, Stored::Secret(GithubLogin::SignIn(kept))));
    assert_eq!(std::fs::read_dir(dir.path().join("github-logins")).unwrap().count(), 1, "and no other file");
    let held_login = change.account.held_login.unwrap();
    let when = (held_login.connected_at_ms, held_login.ends_at_ms, held_login.needs_sign_in);
    assert_eq!(when, (1000, Some(renewed_ends), false), "when it was signed in stays");
    assert!(change.changed);
    assert!(following.has_changed().unwrap());
    assert_eq!(following.borrow_and_update().for_workers(), as_ada("a-renewed-token"));
    assert_eq!(due_at(&logins, ENDS + 1), [], "it isn't due past its old end");
    assert!(!following.has_changed().unwrap(), "and workers are sent the same then");
}

#[test]
fn a_sign_in_whose_refresh_token_github_refused_needs_signing_in_again_and_a_default_it_is_blocks_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    ada_signed_in(&logins, &store, login_ending("a-token", ENDS));
    let due = only_due(&logins, ENDS - 1);
    let before = held(&logins, &store, "g");

    let change = logins.renewed(&due, refused(), ENDS - 1).unwrap().unwrap();

    assert!(change.account.held_login.unwrap().needs_sign_in);
    assert_eq!(held(&logins, &store, "g"), before, "its file is as it was");
    let reason = reason(sent(&logins));
    assert!(reason.contains("ada, has ended") && reason.contains("Sign in with GitHub again"), "{reason}");
    assert_eq!(due_at(&logins, ENDS - 1), [], "it isn't renewed again");
}

/// GitHub revoked those the file holds as it gave them, so they can't be used either.
#[test]
fn renewed_tokens_that_cant_be_kept_have_the_account_need_signing_in_again() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    ada_signed_in(&logins, &store, login_ending("a-token", ENDS));
    let due = only_due(&logins, ENDS - 1);
    let before = held(&logins, &store, "g");
    // Where the renewed tokens are written, beside the file, which can't be made a file, even as root.
    std::fs::create_dir(dir.path().join("github-logins").join(format!("{}.partial", before.0.as_str()))).unwrap();

    let change = logins.renewed(&due, renewed(), ENDS - 1).unwrap().unwrap();

    assert!(change.account.held_login.unwrap().needs_sign_in);
    assert_eq!(held(&logins, &store, "g"), before);
    let reason = reason(sent(&logins));
    assert!(reason.contains("Sign in with GitHub again"), "{reason}");
}

/// Disconnected, signed in again, and renewed already, as when GitHub's answer to a renewal of the tokens it revoked
/// comes after the renewal that revoked them.
#[test]
fn what_renewing_came_to_is_let_go_of_once_the_account_holds_another_login_or_none() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let grace = owner(8, "grace", None);
    ada_signed_in(&logins, &store, login_ending("adas-token", ENDS));
    sign_in_kept(&logins, &store, &grace, "h", login_ending("graces-token", ENDS));
    sign_in_kept(&logins, &store, &owner(9, "lin", None), "i", login_ending("lins-token", ENDS));
    let due = logins.check(ENDS - 1).unwrap().due;
    let [disconnected, signed_in_again, renewed_already] = <[Due; 3]>::try_from(due).unwrap();
    logins.disconnect(&"g".into(), ENDS - 1).unwrap();
    sign_in_kept(&logins, &store, &grace, "unused", login_ending("graces-new-token", ENDS + HOUR));
    logins.renewed(&renewed_already, renewed(), ENDS - 1).unwrap().unwrap();
    let records = || store.list_records().unwrap().github_accounts;
    let (records_before, h_before, i_before) = (records(), held(&logins, &store, "h"), held(&logins, &store, "i"));

    let after = [
        logins.renewed(&disconnected, renewed(), ENDS - 1).unwrap(),
        logins.renewed(&signed_in_again, renewed(), ENDS - 1).unwrap(),
        logins.renewed(&signed_in_again, refused(), ENDS - 1).unwrap(),
        logins.renewed(&renewed_already, refused(), ENDS - 1).unwrap(),
    ];

    assert!(after.iter().all(Option::is_none), "{after:?}");
    assert_eq!(records(), records_before, "no record changed");
    assert_eq!(store.github_login_file(&"g".into()).unwrap(), None, "the one disconnected holds none still");
    let Stored::Secret(GithubLogin::SignIn(signed_in)) = &h_before.1 else { panic!("{h_before:?}") };
    assert_eq!(signed_in.token.as_str(), "graces-new-token", "the one signed in again holds the new sign-in's");
    assert_eq!((held(&logins, &store, "h"), held(&logins, &store, "i")), (h_before, i_before));
    assert_eq!(std::fs::read_dir(dir.path().join("github-logins")).unwrap().count(), 2, "and no other file is kept");
}
