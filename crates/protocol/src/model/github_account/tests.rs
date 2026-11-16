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

fn any_account() -> impl Strategy<Value = GithubAccount> {
    let how = prop_oneof![Just(GithubLoginKind::Token), Just(GithubLoginKind::SignIn)];
    let held_login = (how, any::<i64>(), proptest::option::of(any::<i64>()), any::<bool>()).prop_map(
        |(how, connected_at_ms, ends_at_ms, needs_sign_in)| HeldGithubLogin {
            how,
            connected_at_ms,
            ends_at_ms,
            needs_sign_in,
        },
    );
    let name = "[a-z][a-z0-9-]{0,38}".prop_map(GithubUsername::from);
    (any::<u64>(), name, proptest::option::of(held_login), any::<u64>()).prop_map(|(user, name, held_login, rev)| {
        GithubAccount {
            id: GithubAccountId::from("6f1c2a9e-made-up"),
            user: GithubUser { id: GithubUserId::new(user), name },
            held_login,
            rev: Rev::new(rev),
        }
    })
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// Held or disconnected, ending or not.
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
