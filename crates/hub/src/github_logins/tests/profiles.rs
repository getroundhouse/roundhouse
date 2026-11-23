//! Who a token's and a sign-in's account commits as, from GitHub's profile of its user: as it's connected or signed in,
//! and as a check has GitHub asked again, under the logins' lock. What GitHub's answer says is tested in
//! the `github` crate, and asking it as the hub's check does in `crate::hub`.

use std::path::Path;

use process::testing::{git, git_in};
use protocol::client::ApiError;
use protocol::worker::{GitCredential, GitIdentity, GithubCredential};

use super::credential::{reason, sent};
use super::sign_ins::{ada_signed_in, login_ending, sign_in_kept};
use super::*;
use crate::github_logins::NOBODY_ASKS_AN_HOUR;
use crate::hub::GithubUsername;
use github::GithubProfile;

/// [`PROFILE_READ_EVERY`], in milliseconds.
fn a_day_ms() -> i64 {
    i64::try_from(PROFILE_READ_EVERY.as_millis()).unwrap()
}

/// The GitHub user `name`, of the number `user`, whose token doesn't end, whose profile says it's `named` and shows
/// `email`.
fn owner_with(user: u64, name: &str, named: Option<&str>, email: Option<&str>) -> TokenOwner {
    let profile = GithubProfile { name: named.map(str::to_string), email: email.map(str::to_string) };
    TokenOwner { profile, ..owner(user, name, None) }
}

/// Who `credential` commits as, as `name <email>`, or `None` for one that carries no commit identity. Fails for
/// anything but an account's.
fn commits_as(credential: &GithubCredential) -> Option<String> {
    let GithubCredential::Account(GitCredential { identity, .. }) = credential else { panic!("{credential:?}") };
    identity.as_ref().map(|GitIdentity { name, email }| format!("{name} <{email}>"))
}

#[test]
fn a_default_connected_by_a_token_or_signed_in_commits_as_its_name_and_public_email_on_github() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let ada = owner_with(7, "ada", Some("Ada Lovelace"), Some("ada@example.com"));
    let grace = owner_with(8, "grace", Some("Grace Hopper"), Some("grace@example.com"));

    logins.connected(&connect("a-token", None, "g1"), &ada).unwrap();
    let by_token = commits_as(&sent(&logins));
    let signed_in = sign_in_kept(&logins, &store, &grace, "g2", login_ending("b-token", 9000));
    logins.set_default(Some(&signed_in), 2000).unwrap();

    assert_eq!(by_token.as_deref(), Some("Ada Lovelace <ada@example.com>"));
    assert_eq!(commits_as(&sent(&logins)).as_deref(), Some("Grace Hopper <grace@example.com>"));
}

/// Connected again each time, with another token of hers, as her profile on GitHub changes.
#[test]
fn a_name_falls_back_to_the_username_and_an_email_to_the_address_github_gives_the_user() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());

    logins.connected(&connect("first", None, "g1"), &owner_with(7, "ada", None, None)).unwrap();
    let neither = commits_as(&sent(&logins));
    logins.connected(&connect("second", Some("g1"), "g2"), &owner_with(7, "ada", Some("Ada Lovelace"), None)).unwrap();
    let only_a_name = commits_as(&sent(&logins));
    logins
        .connected(&connect("third", Some("g1"), "g3"), &owner_with(7, "ada", None, Some("ada@example.com")))
        .unwrap();
    let only_an_email = commits_as(&sent(&logins));

    assert_eq!(neither.as_deref(), Some("ada <7+ada@users.noreply.github.com>"));
    assert_eq!(only_a_name.as_deref(), Some("Ada Lovelace <7+ada@users.noreply.github.com>"));
    assert_eq!(only_an_email.as_deref(), Some("ada <ada@example.com>"));
}

/// Names made only of what git takes out of a name, or trims from its ends, which git 2.31 won't commit as, then some
/// with more, which git commits as with that taken out.
const NAMES: [&str; 7] = [".", "<>", r#" .,:;"\' "#, "\n<\t>\n", "Ada.", "<Ada>", "🚀"];

/// Who ada commits as, as workers are sent it, while her profile on GitHub says she's named each of [`NAMES`] in turn.
fn identities_of_each_name() -> Vec<GitIdentity> {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connected(&connect("a-token", None, "g1"), &owner_with(7, "ada", None, None)).unwrap();
    let mut identities = Vec::new();
    for (at, name) in NAMES.into_iter().enumerate() {
        let token = format!("token-{at}");
        logins.connected(&connect(&token, Some("g1"), "unused"), &owner_with(7, "ada", Some(name), None)).unwrap();
        let GithubCredential::Account(credential) = sent(&logins) else { panic!("{:?}", sent(&logins)) };
        identities.push(credential.identity.unwrap());
    }
    identities
}

/// Whether git commits in `repo` as `name` and `email`, given in its config, as a worker gives them.
fn commits_through_git(repo: &Path, name: &str, email: &str) -> bool {
    let mut command = git_in(repo);
    // `git_in` names a made-up author in the environment, which git would take over the config.
    for author in ["GIT_AUTHOR_NAME", "GIT_AUTHOR_EMAIL", "GIT_COMMITTER_NAME", "GIT_COMMITTER_EMAIL"] {
        command.env_remove(author);
    }
    let (name, email) = (format!("user.name={name}"), format!("user.email={email}"));
    command.args(["-c", &name, "-c", &email, "commit", "--allow-empty", "--quiet", "-m", "made up"]);
    command.output().unwrap().status.success()
}

/// Those that are only what git takes out are sent as the username, and the rest as GitHub gives them. Then through
/// the git on this machine, as an agent's commits are. Git 2.31 refuses every name that falls back, but a later one,
/// such as 2.54, keeps `.`, and so commits as a name with one, so only those without are refused by every git.
#[test]
fn a_name_git_wont_commit_as_falls_back_to_the_username_and_git_commits_as_every_name_sent() {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "--quiet"]);

    let identities = identities_of_each_name();

    let named: Vec<&str> = identities.iter().map(|identity| identity.name.as_str()).collect();
    assert_eq!(named, ["ada", "ada", "ada", "ada", "Ada.", "<Ada>", "🚀"]);
    for (name, sent) in NAMES.into_iter().zip(identities) {
        assert!(commits_through_git(repo.path(), &sent.name, &sent.email), "git refused {:?}", sent.name);
        if sent.name != name && !name.contains('.') {
            assert!(!commits_through_git(repo.path(), name, &sent.email), "git took {name:?}, which fell back");
        }
    }
}

/// Her name, her email and her username changed on GitHub. What the hub heard is answered with, for the hub to keep, and
/// once it has, workers are sent what it makes of the default.
#[test]
fn a_check_a_day_on_has_github_asked_again_and_whats_changed_there_reaches_workers() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connected(&connect("a-token", None, "g1"), &owner_with(7, "ada", Some("Ada"), None)).unwrap();
    let mut following = logins.git_credential();
    following.borrow_and_update();

    let before_a_day = logins.check(1000 + a_day_ms() - 1).unwrap().profiles;
    let mut a_day_on = logins.check(1000 + a_day_ms()).unwrap().profiles;
    let due = a_day_on.pop().unwrap();
    let changed = owner_with(7, "ada-lovelace", Some("Ada Lovelace"), Some("ada@example.com"));
    let heard = logins.profile_read(&due, Ok(changed.clone()), 1000 + a_day_ms()).unwrap().unwrap();
    super::heard(&logins, &heard.account, &changed, heard.at_ms);

    assert!(before_a_day.is_empty(), "{before_a_day:?}");
    assert!(matches!(&due.from, ReadFrom::Github(token) if token.as_str() == "a-token"), "{due:?}");
    let profile = Some(changed.profile.clone());
    assert_eq!((heard.username.as_str(), heard.profile), ("ada-lovelace", profile));
    let credential = following.borrow_and_update().for_workers();
    assert_eq!(commits_as(&credential).as_deref(), Some("Ada Lovelace <ada@example.com>"));
    assert!(
        matches!(credential, GithubCredential::Account(GitCredential { username, .. }) if username.as_str() == "ada-lovelace")
    );
}

/// As on the first start after the hub stopped keeping them in its database. Until the hub has heard what GitHub calls
/// the default's user, workers are sent a block, which no check of reach reads as the default's, and the account is
/// due to be asked at once. Once the username is known, and the profile isn't, she commits as the address GitHub gives
/// her, never as nobody.
#[test]
fn an_account_whose_user_isnt_known_is_asked_at_once_and_blocks_until_then_and_its_profile_falls_back_meanwhile() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connect(&connect("a-token", None, "g1"), &owner_with(7, "ada", Some("Ada"), None)).unwrap();

    let unnamed = logins.git_credential().borrow().clone();
    let due = logins.check(1001).unwrap().profiles;
    logins.observations.keep_as_read::<GithubUsername>("g1".into(), "ada".into(), 1001);
    logins.heard_of(1001).unwrap();

    assert!(matches!(unnamed, DefaultCredential::Unnamed { .. }), "{unnamed:?}");
    assert!(reason(unnamed.for_workers()).starts_with("Roundhouse is asking GitHub who"));
    assert_eq!(due.iter().map(|due| due.account.as_str()).collect::<Vec<_>>(), ["g1"]);
    assert_eq!(commits_as(&sent(&logins)).as_deref(), Some("ada <7+ada@users.noreply.github.com>"));
    assert_eq!(logins.check(1002).unwrap().profiles.len(), 1, "its profile is still to be read");
}

/// No answer, and GitHub naming another user, keep nothing, so the next check asks again. GitHub refusing the token, as
/// one revoked, has the next check ask with no login, until the account is connected again, and what GitHub said
/// before stands.
#[test]
fn a_token_github_refuses_is_asked_about_with_no_login_and_any_other_failure_with_it_at_the_next_check() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connected(&connect("a-token", None, "g1"), &owner_with(7, "ada", Some("Ada"), None)).unwrap();
    let at = 1000 + a_day_ms();
    let due_at = |now_ms| logins.check(now_ms).unwrap().profiles.pop();

    let first = due_at(at).unwrap();
    logins.profile_read(&first, Err(ApiError::new(ErrorCode::Unavailable, "no answer")), at).unwrap();
    let after_no_answer = due_at(at + 1).unwrap();
    logins.profile_read(&after_no_answer, Ok(owner_with(8, "grace", Some("Grace"), None)), at + 1).unwrap();
    let after_another_user = due_at(at + 2).unwrap();
    logins.profile_read(&after_another_user, Err(ApiError::new(ErrorCode::InvalidRequest, "revoked")), at + 2).unwrap();
    let after_a_refusal = due_at(at + 3).unwrap();
    logins
        .connected(&Connect { connected_at_ms: at + 4, ..connect("b-token", Some("g1"), "g2") }, &owner(7, "ada", None))
        .unwrap();
    let connected_again = due_at(at + 4 + a_day_ms()).unwrap();

    assert!(matches!(after_no_answer.from, ReadFrom::Github(_)), "{after_no_answer:?}");
    assert!(matches!(after_another_user.from, ReadFrom::Github(_)), "{after_another_user:?}");
    assert_eq!(after_a_refusal.from, ReadFrom::Nobody);
    assert!(
        matches!(&connected_again.from, ReadFrom::Github(token) if token.as_str() == "b-token"),
        "{connected_again:?}"
    );
}

/// A sign-in the check renews, whose token GitHub revokes as it gives the next, is asked about at the next check, with
/// that one. A token that has ended can't be asked with, so it's asked about with no login.
#[test]
fn a_check_leaves_a_sign_in_it_renews_for_the_next_and_asks_with_no_login_of_an_ended_token() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let now_ms = 1000 + a_day_ms();
    ada_signed_in(&logins, &store, login_ending("a-token", now_ms + 1));
    logins.connected(&connect("b-token", None, "g2"), &owner(8, "grace", Some(now_ms))).unwrap();

    let checked = logins.check(now_ms).unwrap();

    assert_eq!(checked.due.len(), 1, "ada's sign-in is renewed");
    let asked: Vec<(&str, &ReadFrom)> = checked.profiles.iter().map(|due| (due.account.as_str(), &due.from)).collect();
    assert_eq!(asked, [("g2", &ReadFrom::Nobody)]);
}

/// However many accounts can't be asked about with their own login, the hub asks GitHub with no login at most
/// [`NOBODY_ASKS_AN_HOUR`] times an hour, leaving the rest for later in the hour after, and none for an hour once GitHub
/// says it's limiting such calls, each then not known as it was.
#[test]
fn asks_with_no_login_are_kept_well_under_githubs_limit_and_stop_for_an_hour_once_it_limits_them() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    for n in 0..30u64 {
        let id = format!("g{n}");
        logins.connected(&connect(&format!("token-{n}"), None, &id), &owner(100 + n, "user", Some(1001))).unwrap();
        logins.observations.forget_for_a_test::<crate::hub::GithubUsername>(&id.as_str().into());
    }
    let hour_ms = 60 * 60 * 1000;

    let first = logins.check(2000).unwrap().profiles;
    let in_the_hour = logins.check(2000 + hour_ms - 1).unwrap().profiles;
    let an_hour_on = logins.check(2000 + hour_ms).unwrap().profiles;
    let limited = &an_hour_on[0];
    let heard = logins.nobody_read(limited, Err(github::NotRead::Limited), 2000 + hour_ms).unwrap();
    let while_limited = logins.check(2000 + 2 * hour_ms - 1).unwrap().profiles;
    let after = logins.check(2000 + 2 * hour_ms).unwrap().profiles;

    assert!(first.iter().all(|due| due.from == ReadFrom::Nobody));
    assert_eq!((first.len(), in_the_hour.len(), an_hour_on.len()), (NOBODY_ASKS_AN_HOUR, 0, NOBODY_ASKS_AN_HOUR));
    assert_eq!(heard, None, "not known, and nothing decided");
    assert_eq!(while_limited.len(), 0);
    assert_eq!(after.len(), NOBODY_ASKS_AN_HOUR);
}
