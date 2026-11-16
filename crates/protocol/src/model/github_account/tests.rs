use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use serde_json::json;

use super::*;

fn any_account() -> impl Strategy<Value = GithubAccount> {
    let held_login = (any::<i64>(), proptest::option::of(any::<i64>())).prop_map(|(connected_at_ms, ends_at_ms)| {
        HeldGithubLogin { how: GithubLoginKind::Token, connected_at_ms, ends_at_ms }
    });
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
fn a_login_works_until_it_ends() {
    let ends = HeldGithubLogin { how: GithubLoginKind::Token, connected_at_ms: 0, ends_at_ms: Some(10) };

    assert_eq!([9, 10, 11].map(|now_ms| ends.works_at(now_ms)), [true, false, false]);
}
