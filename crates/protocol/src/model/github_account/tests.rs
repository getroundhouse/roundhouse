use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use serde_json::json;

use super::*;

fn any_account() -> impl Strategy<Value = GithubAccount> {
    let how = prop_oneof![Just(GithubLoginKind::SignIn), Just(GithubLoginKind::Token), Just(GithubLoginKind::App)];
    let held_login = (how, any::<i64>(), proptest::option::of(any::<i64>()), any::<bool>()).prop_map(
        |(how, connected_at_ms, ends_at_ms, needs_sign_in)| HeldGithubLogin {
            how,
            connected_at_ms,
            ends_at_ms,
            needs_sign_in,
        },
    );
    (any::<u64>(), username(), held_login, any::<u64>()).prop_map(|(user, name, held_login, rev)| GithubAccount {
        id: GithubAccountId::from("6f1c2a9e-made-up"),
        user: GithubUser { id: GithubUserId::new(user), name },
        held_login,
        rev: Rev::new(rev),
    })
}

fn username() -> impl Strategy<Value = GithubUsername> {
    "[a-z][a-z0-9-]{0,38}".prop_map(GithubUsername::from)
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    #[test]
    fn an_accounts_record_reads_back_as_it_was(account in any_account()) {
        let written = serde_json::to_string(&account).unwrap();

        prop_assert_eq!(serde_json::from_str::<GithubAccount>(&written).unwrap(), account);
    }

    /// Logged in as someone, and asked and found logged in to nothing, each read back as themselves.
    #[test]
    fn a_found_login_reads_back_as_it_was(
        provider in "[a-z]{1,8}",
        username in proptest::option::of(username()),
        rev in any::<u64>(),
    ) {
        let found = FoundGithubLogin {
            provider: ProviderName::from(provider),
            login: username.map(|username| GithubLogin { username }),
            rev: Rev::new(rev),
        };

        let written = serde_json::to_string(&found).unwrap();

        prop_assert_eq!(serde_json::from_str::<FoundGithubLogin>(&written).unwrap(), found);
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

    assert_eq!(account.held_login.how, GithubLoginKind::Unknown);
}

#[test]
fn an_account_that_says_only_who_it_is_and_how_its_login_is_held_needs_nothing_and_never_ends() {
    let said =
        json!({"id": "g", "user": {"id": 7, "name": "ada"}, "held_login": {"how": "token", "connected_at_ms": 1}});

    let account: GithubAccount = serde_json::from_value(said).unwrap();

    assert_eq!((account.user.id, account.user.name.as_str()), (GithubUserId::new(7), "ada"));
    assert_eq!((account.held_login.ends_at_ms, account.held_login.needs_sign_in), (None, false));
    assert_eq!(account.rev, Rev::default());
}
