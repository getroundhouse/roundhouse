//! Who a token's and a sign-in's account commits as, from GitHub's profile of its user: as it's connected or signed in,
//! and as a check has GitHub asked again, under the logins' lock. What GitHub's answer says is tested in
//! `crate::github`, and asking it as the hub's check does in `crate::hub`.

use std::path::Path;

use process::testing::{git, git_in};
use protocol::client::ApiError;
use protocol::worker::{GitCredential, GitIdentity, GithubCredential};

use super::credential::sent;
use super::sign_ins::{ada_signed_in, login_ending, sign_in_kept};
use super::*;
use crate::login_files::LoginFile;
use crate::store::GithubProfile;
use crate::store::older::{BEFORE_GITHUB_PROFILES, store_from_before};

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

    logins.connect(&connect("a-token", None, "g1"), &ada).unwrap();
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

    logins.connect(&connect("first", None, "g1"), &owner_with(7, "ada", None, None)).unwrap();
    let neither = commits_as(&sent(&logins));
    logins.connect(&connect("second", Some("g1"), "g2"), &owner_with(7, "ada", Some("Ada Lovelace"), None)).unwrap();
    let only_a_name = commits_as(&sent(&logins));
    logins.connect(&connect("third", Some("g1"), "g3"), &owner_with(7, "ada", None, Some("ada@example.com"))).unwrap();
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
    logins.connect(&connect("a-token", None, "g1"), &owner_with(7, "ada", None, None)).unwrap();
    let mut identities = Vec::new();
    for (at, name) in NAMES.into_iter().enumerate() {
        let token = format!("token-{at}");
        logins.connect(&connect(&token, Some("g1"), "unused"), &owner_with(7, "ada", Some(name), None)).unwrap();
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

/// Those that are only what git takes out are sent as the username, and the rest as GitHub gives them.
#[test]
fn a_name_git_wont_commit_as_falls_back_to_the_username() {
    let named: Vec<String> = identities_of_each_name().into_iter().map(|identity| identity.name).collect();

    assert_eq!(named, ["ada", "ada", "ada", "ada", "Ada.", "<Ada>", "🚀"]);
}

/// Through the git on this machine, as an agent's commits are. Git 2.31 refuses every name that falls back, but a
/// later one, such as 2.54, keeps `.`, and so commits as a name with one, so only those without are refused by every
/// git.
#[test]
fn git_commits_as_every_name_sent_and_refuses_each_that_falls_back() {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "--quiet"]);

    let identities = identities_of_each_name();

    for (name, sent) in NAMES.into_iter().zip(identities) {
        assert!(commits_through_git(repo.path(), &sent.name, &sent.email), "git refused {:?}", sent.name);
        if sent.name != name && !name.contains('.') {
            assert!(!commits_through_git(repo.path(), name, &sent.email), "git took {name:?}, which fell back");
        }
    }
}

/// As a hub from before profiles left it: ada's token in its file, the default, and never asked who she is.
#[test]
fn an_account_an_older_hub_connected_commits_as_the_machines_git_says_until_a_check_has_github_asked_who_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let file = LoginFile::new();
    std::fs::create_dir(dir.path().join("github-logins")).unwrap();
    std::fs::write(dir.path().join("github-logins").join(file.as_str()), "a-token").unwrap();
    let held = format!(
        "INSERT INTO github_accounts (id, user_id, username, login_kind, login_file, login_connected_at_ms)
             VALUES ('g', 7, 'ada', 'token', '{}', 1);
         UPDATE hub_settings SET github_account_id = 'g';",
        file.as_str()
    );
    let logins =
        GithubLogins::new(Arc::new(store_from_before(dir.path(), BEFORE_GITHUB_PROFILES, &held)), dir.path(), 1000);
    let as_the_hub_started = sent(&logins);

    let mut first = logins.check(1000).unwrap().profiles;
    let due = first.pop().unwrap();
    logins.profile_read(&due, Ok(owner_with(7, "ada", Some("Ada Lovelace"), None)), 1000).unwrap();
    let once_asked = sent(&logins);
    let next = logins.check(1001).unwrap().profiles;

    let GithubCredential::Account(credential) = &as_the_hub_started else { panic!("{as_the_hub_started:?}") };
    assert_eq!((credential.token.as_str(), &credential.identity), ("a-token", &None));
    assert_eq!((first.len(), due.token.as_str()), (0, "a-token"), "only ada was due, with her token");
    assert_eq!(commits_as(&once_asked).as_deref(), Some("Ada Lovelace <7+ada@users.noreply.github.com>"));
    assert!(next.is_empty(), "it isn't asked about again until a day on");
}

/// Her name, her email and her username changed on GitHub. The username is on the account's record, which every client
/// follows, so it's answered with.
#[test]
fn a_check_a_day_on_has_github_asked_again_and_whats_changed_there_reaches_workers_and_a_new_username_the_record() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connect(&connect("a-token", None, "g1"), &owner_with(7, "ada", Some("Ada"), None)).unwrap();
    let mut following = logins.git_credential();
    following.borrow_and_update();

    let before_a_day = logins.check(1000 + a_day_ms() - 1).unwrap().profiles;
    let mut a_day_on = logins.check(1000 + a_day_ms()).unwrap().profiles;
    let due = a_day_on.pop().unwrap();
    let changed = owner_with(7, "ada-lovelace", Some("Ada Lovelace"), Some("ada@example.com"));
    let renamed = logins.profile_read(&due, Ok(changed), 1000 + a_day_ms()).unwrap().unwrap();

    assert!(before_a_day.is_empty(), "{before_a_day:?}");
    assert_eq!(due.token.as_str(), "a-token");
    assert_eq!((renamed.account.user.name.as_str(), renamed.changed), ("ada-lovelace", true));
    let credential = following.borrow_and_update().clone();
    assert_eq!(commits_as(&credential).as_deref(), Some("Ada Lovelace <ada@example.com>"));
    assert!(
        matches!(credential, GithubCredential::Account(GitCredential { username, .. }) if username.as_str() == "ada-lovelace")
    );
}

/// No answer, and GitHub naming another user, keep nothing, so the next check asks again. GitHub refusing the token, as
/// one revoked, is asked about again only a day on, and what GitHub said before stands.
#[test]
fn a_token_github_refuses_is_asked_about_again_only_a_day_on_and_any_other_failure_at_the_next_check() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, _) = logins_in(dir.path());
    logins.connect(&connect("a-token", None, "g1"), &owner_with(7, "ada", Some("Ada"), None)).unwrap();
    let at = 1000 + a_day_ms();
    let due_at = |now_ms| logins.check(now_ms).unwrap().profiles.pop();

    let first = due_at(at).unwrap();
    logins.profile_read(&first, Err(ApiError::new(ErrorCode::Unavailable, "no answer")), at).unwrap();
    let after_no_answer = due_at(at + 1).unwrap();
    logins.profile_read(&after_no_answer, Ok(owner_with(8, "grace", Some("Grace"), None)), at + 1).unwrap();
    let after_another_user = due_at(at + 2).unwrap();
    logins.profile_read(&after_another_user, Err(ApiError::new(ErrorCode::InvalidRequest, "revoked")), at + 2).unwrap();
    let after_a_refusal = due_at(at + 3);
    let a_day_after_it = due_at(at + 2 + a_day_ms());

    assert!(after_a_refusal.is_none(), "{after_a_refusal:?}");
    assert!(a_day_after_it.is_some());
    assert_eq!(commits_as(&sent(&logins)).as_deref(), Some("Ada <7+ada@users.noreply.github.com>"));
}

/// A sign-in the check renews, whose token GitHub revokes as it gives the next, is asked about at the next check, with
/// that one. A token that has ended can't be.
#[test]
fn a_check_leaves_a_sign_in_it_renews_for_the_next_and_has_nothing_asked_of_an_ended_token() {
    let dir = tempfile::tempdir().unwrap();
    let (logins, store) = logins_in(dir.path());
    let now_ms = 1000 + a_day_ms();
    ada_signed_in(&logins, &store, login_ending("a-token", now_ms + 1));
    logins.connect(&connect("b-token", None, "g2"), &owner(8, "grace", Some(now_ms))).unwrap();

    let checked = logins.check(now_ms).unwrap();

    assert_eq!(checked.due.len(), 1, "ada's sign-in is renewed");
    assert!(checked.profiles.is_empty(), "{:?}", checked.profiles);
}
