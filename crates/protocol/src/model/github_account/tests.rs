use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use serde_json::json;

use super::*;

/// A sign-in in any state but one from a newer hub, which this build can't write.
fn any_sign_in() -> impl Strategy<Value = GithubSignIn> {
    let waiting =
        ("[A-Z0-9]{4}-[A-Z0-9]{4}", any::<i64>()).prop_map(|(code, code_ends_at_ms)| GithubSignInStatus::Waiting {
            code: GithubUserCode::from(code),
            address: "https://github.com/login/device".to_string(),
            code_ends_at_ms,
        });
    let status = prop_oneof![
        waiting,
        Just(GithubSignInStatus::Approved { account: GithubAccountId::from("6f1c2a9e-made-up") }),
        Just(GithubSignInStatus::Refused),
        Just(GithubSignInStatus::Ended),
        Just(GithubSignInStatus::Cancelled),
        ".{0,40}".prop_map(|message| GithubSignInStatus::Failed { message }),
    ];
    (status, any::<u64>()).prop_map(|(status, rev)| GithubSignIn {
        id: GithubSignInId::from("a-clients-own"),
        status,
        rev: Rev::new(rev),
    })
}

fn any_user() -> impl Strategy<Value = GithubUser> {
    (any::<u64>(), "[a-z][a-z0-9-]{0,38}(\\[bot\\])?")
        .prop_map(|(id, name)| GithubUser { id: GithubUserId::new(id), name: GithubUsername::from(name) })
}

fn any_installation() -> impl Strategy<Value = GithubInstallation> {
    (any::<u64>(), any::<u64>(), any_user()).prop_map(|(app, id, on)| GithubInstallation {
        app: GithubAppId::new(app),
        id: GithubInstallationId::new(id),
        on,
    })
}

fn any_account() -> impl Strategy<Value = GithubAccount> {
    let how = prop_oneof![Just(GithubLoginKind::Token), Just(GithubLoginKind::SignIn), Just(GithubLoginKind::App)];
    let held_login = (how, any::<i64>(), proptest::option::of(any::<i64>()), any::<bool>()).prop_map(
        |(how, connected_at_ms, ends_at_ms, needs_sign_in)| HeldGithubLogin {
            how,
            connected_at_ms,
            ends_at_ms,
            needs_sign_in,
        },
    );
    let installation = proptest::option::of(any_installation());
    (any_user(), proptest::option::of(held_login), installation, any::<u64>()).prop_map(
        |(user, held_login, installation, rev)| GithubAccount {
            id: GithubAccountId::from("6f1c2a9e-made-up"),
            user,
            held_login,
            installation,
            rev: Rev::new(rev),
        },
    )
}

fn any_app() -> impl Strategy<Value = GithubApp> {
    let slug = "[a-z][a-z0-9-]{0,33}".prop_map(GithubAppSlug::from);
    (any::<u64>(), slug, ".{0,34}", any_user(), proptest::option::of(any_user()), any::<u64>()).prop_map(
        |(id, slug, name, owner, bot, rev)| GithubApp {
            id: GithubAppId::new(id),
            slug,
            name,
            owner,
            bot,
            rev: Rev::new(rev),
        },
    )
}

fn any_gh_login() -> impl Strategy<Value = GhLogin> {
    let works = prop_oneof![
        Just(GhLoginWorks::Yes),
        Just(GhLoginWorks::Refused),
        Just(GhLoginWorks::CouldntTell),
        Just(GhLoginWorks::Unknown),
    ];
    let logged_in = ("[a-z][a-z0-9-]{0,38}", works)
        .prop_map(|(name, works)| GhLogin::LoggedIn { username: GithubUsername::from(name), works });
    prop_oneof![logged_in, Just(GhLogin::LoggedOut), Just(GhLogin::NotInstalled)]
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// Logged in as someone, logged in to nothing, and with no `gh` there.
    #[test]
    fn a_found_login_reads_back_as_it_was(provider in "[a-z]{1,8}", login in any_gh_login(), rev in any::<u64>()) {
        let found = FoundGithubLogin { provider: ProviderName::from(provider), login, rev: Rev::new(rev) };

        let written = serde_json::to_string(&found).unwrap();

        prop_assert_eq!(serde_json::from_str::<FoundGithubLogin>(&written).unwrap(), found);
    }

    /// Whoever's bot it is, numbered or not.
    #[test]
    fn an_apps_record_reads_back_as_it_was(app in any_app()) {
        let written = serde_json::to_string(&app).unwrap();

        prop_assert_eq!(serde_json::from_str::<GithubApp>(&written).unwrap(), app);
    }

    /// Held or disconnected, ending or not, an installation or not.
    #[test]
    fn an_accounts_record_reads_back_as_it_was(account in any_account()) {
        let written = serde_json::to_string(&account).unwrap();

        prop_assert_eq!(serde_json::from_str::<GithubAccount>(&written).unwrap(), account);
    }

    /// Whichever state it's in.
    #[test]
    fn a_sign_ins_record_reads_back_as_it_was(sign_in in any_sign_in()) {
        let written = serde_json::to_string(&sign_in).unwrap();

        prop_assert_eq!(serde_json::from_str::<GithubSignIn>(&written).unwrap(), sign_in);
    }

    /// Whatever the token, as GitHub's own kinds of token start.
    #[test]
    fn a_token_goes_on_the_wire_as_it_is_and_never_into_debug(token in "(ghp_|github_pat_|gho_|ghu_)[A-Za-z0-9_]{8,60}") {
        let held = GithubToken::new(token.clone());

        let read: GithubToken = serde_json::from_value(json!(token)).unwrap();

        prop_assert_eq!(serde_json::to_value(&held).unwrap(), json!(token));
        prop_assert_eq!(read.as_str(), token.as_str());
        prop_assert!(!format!("{held:?}").contains(&token), "{:?}", held);
    }
}

#[test]
fn a_login_held_in_a_way_from_a_newer_hub_reads_as_unknown() {
    let newer =
        json!({"id": "g", "user": {"id": 7, "name": "ada"}, "held_login": {"how": "passkey", "connected_at_ms": 1}});

    let account: GithubAccount = serde_json::from_value(newer).unwrap();

    assert_eq!(account.held_login.map(|held| held.how), Some(GithubLoginKind::Unknown));
}

#[test]
fn an_account_that_says_only_who_it_is_holds_no_login_and_one_that_says_no_end_never_ends() {
    let disconnected = json!({"id": "g", "user": {"id": 7, "name": "ada"}});
    let held =
        json!({"id": "g", "user": {"id": 7, "name": "ada"}, "held_login": {"how": "token", "connected_at_ms": 1}});

    let disconnected: GithubAccount = serde_json::from_value(disconnected).unwrap();
    let held: GithubAccount = serde_json::from_value(held).unwrap();

    assert_eq!((disconnected.user.id, disconnected.user.name.as_str()), (GithubUserId::new(7), "ada"));
    assert_eq!((disconnected.held_login, disconnected.rev), (None, Rev::default()));
    assert_eq!(held.held_login.and_then(|held| held.ends_at_ms), None);
    assert!(held.held_login.unwrap().works_at(i64::MAX));
}

#[test]
fn a_login_works_until_it_ends_or_needs_signing_in_again() {
    let ends =
        HeldGithubLogin { how: GithubLoginKind::Token, connected_at_ms: 0, ends_at_ms: Some(10), needs_sign_in: false };
    let needs_sign_in = HeldGithubLogin { how: GithubLoginKind::SignIn, ends_at_ms: None, needs_sign_in: true, ..ends };

    assert_eq!([9, 10, 11].map(|now_ms| ends.works_at(now_ms)), [true, false, false]);
    assert!(!needs_sign_in.works_at(0));
}

#[test]
fn a_signed_in_login_from_a_hub_from_before_it_said_needs_no_sign_in() {
    let older = json!({"how": "sign_in", "connected_at_ms": 1, "ends_at_ms": 9});

    let held: HeldGithubLogin = serde_json::from_value(older).unwrap();

    assert_eq!((held.how, held.needs_sign_in), (GithubLoginKind::SignIn, false));
}

#[test]
fn a_sign_in_in_a_state_from_a_newer_hub_reads_as_unknown_and_isnt_waiting() {
    let newer = json!({"id": "s", "status": {"state": "asking_for_a_passkey", "since_ms": 1}, "rev": 3});

    let sign_in: GithubSignIn = serde_json::from_value(newer).unwrap();

    assert_eq!(sign_in.status, GithubSignInStatus::Unknown);
    assert!(!sign_in.status.is_waiting());
}

/// As a newer worker might say it, through a hub that passes it on, and with no revision, as from a hub that keeps none.
#[test]
fn a_found_login_in_a_state_from_a_newer_peer_reads_as_unknown() {
    let newer = json!({"provider": "local", "login": {"state": "logged_in_twice", "usernames": ["ada", "grace"]}});

    let found: FoundGithubLogin = serde_json::from_value(newer).unwrap();

    assert_eq!((found.login, found.rev), (GhLogin::Unknown, Rev::default()));
}

/// Whether its token works, as a newer worker might say it, and as a peer that doesn't say it leaves it out.
#[test]
fn whether_a_found_login_works_reads_as_unknown_when_its_from_a_newer_peer_or_not_said() {
    let newer = json!({"state": "logged_in", "username": "ada", "works": "only_on_tuesdays"});
    let unsaid = json!({"state": "logged_in", "username": "ada"});

    for login in [newer, unsaid] {
        let login: GhLogin = serde_json::from_value(login).unwrap();
        assert_eq!(login, GhLogin::LoggedIn { username: "ada".into(), works: GhLoginWorks::Unknown });
    }
}

/// As GitHub writes its addresses, and nothing an address could be moved by, or that has no owner or no name.
#[test]
fn a_repository_on_github_is_an_owner_and_a_name_of_githubs_letters() {
    let read = |written: &str| GithubRepositoryName::try_from(written.to_string()).map(String::from);
    let (not_both, not_githubs) = (GithubRepositoryNameError::NotOwnerAndName, GithubRepositoryNameError::NotGithubs);

    assert_eq!(read("cli/cli"), Ok("cli/cli".to_string()));
    assert_eq!(read("get-round_house/app.rs"), Ok("get-round_house/app.rs".to_string()));
    assert_eq!(read(".github/.github"), Ok(".github/.github".to_string()));
    for (written, why) in [
        ("cli", not_both),
        ("cli/", not_both),
        ("/cli", not_both),
        ("cli/cli/pulls", not_both),
        ("", not_both),
        ("cli/..", not_githubs),
        ("./cli", not_githubs),
        ("cli/cl i", not_githubs),
        ("cli/cli?x=1", not_githubs),
        ("cli/clí", not_githubs),
    ] {
        assert_eq!(read(written), Err(why), "{written}");
    }
    assert_eq!(GithubRepositoryName::named("get-round_house/app.rs").owner(), "get-round_house");
}

/// So a newer hub's word reaches no client as one this build knows, and a name that isn't one fails only its record.
#[test]
fn a_reach_in_a_state_from_a_newer_hub_reads_as_unknown_and_one_naming_no_repository_isnt_read() {
    let newer = json!({"repo": "r", "reach": {"state": "pushes_sometimes", "repository": "cli/cli"}});
    let not_one = json!({"repo": "r", "reach": {"state": "pushes", "repository": "cli", "username": "ada"}});

    let read: GithubReach = serde_json::from_value(newer).unwrap();

    assert_eq!((read.reach, read.rev), (Reach::Unknown, Rev::default()));
    assert!(serde_json::from_value::<GithubReach>(not_one).is_err());
}
