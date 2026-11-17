use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};

use super::*;
use crate::model::GithubRepositoryName;

fn url(text: &str) -> Result<String, RepoUrlError> {
    RepoUrl::try_from(text.to_owned()).map(String::from)
}

fn without_credential(text: &str) -> Option<String> {
    RepoUrl::without_credential(text).map(String::from)
}

#[test]
fn an_address_keeps_the_user_ssh_logs_in_as_and_nothing_else_before_its_host() {
    let kept = [
        "https://github.com/ada/roundhouse.git",
        "git@github.com:ada/roundhouse.git",
        "ssh://git@github.com/ada/roundhouse.git",
        "git+ssh://git@github.com:22/ada/roundhouse.git",
        "file:///Users/ada/roundhouse",
        "/Users/ada/roundhouse",
        "https://github.com/ada/roundhouse@main",
    ];

    for address in kept {
        assert_eq!(url(address), Ok(address.to_owned()), "{address}");
        assert_eq!(without_credential(address), Some(address.to_owned()), "{address}");
    }
}

#[test]
fn a_credential_is_cut_out_of_an_address_read_and_refused_anywhere_else() {
    let cut = [
        ("https://ada:ghp_made_up@github.com/ada/roundhouse.git", "https://github.com/ada/roundhouse.git"),
        ("https://ghp_made_up@github.com/ada/roundhouse.git", "https://github.com/ada/roundhouse.git"),
        ("HTTP://x-access-token:made@up@github.com", "HTTP://github.com"),
        ("git://ada:made-up@example.com/repo.git", "git://example.com/repo.git"),
        ("ssh://git:made-up@github.com/ada/roundhouse.git", "ssh://git@github.com/ada/roundhouse.git"),
        ("SSH://git:made:up@github.com/ada/roundhouse.git", "SSH://git@github.com/ada/roundhouse.git"),
    ];

    for (read, kept) in cut {
        assert_eq!(without_credential(read).as_deref(), Some(kept), "{read}");
        assert_eq!(url(read), Err(RepoUrlError::HoldsCredential), "{read}");
    }
    assert_eq!((url(""), without_credential("")), (Err(RepoUrlError::Empty), None));
}

/// The text is a credential, so neither a refusal nor what serde says of it holds the text.
#[test]
fn a_refusal_never_says_what_it_refused() {
    let read = serde_json::from_str::<RepoUrl>(r#""https://ada:ghp_made_up@github.com/ada/roundhouse.git""#);

    let said = read.unwrap_err().to_string();
    assert!(said.contains(&RepoUrlError::HoldsCredential.to_string()), "{said}");
    assert!(!said.contains("ghp_made_up"), "{said}");
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// So what a worker reads, once it's cut, is always one the hub keeps.
    #[test]
    fn whatever_is_read_is_kept_once_its_credential_is_cut(
        scheme in "(https?|ssh|git\\+ssh|git|file|HTTPS|Ssh)?(://)?",
        rest in "[a-z:@/.?#_-]{0,40}",
    ) {
        let read = format!("{scheme}{rest}");
        if let Some(cut) = RepoUrl::without_credential(&read) {
            prop_assert_eq!(RepoUrl::try_from(cut.to_string()), Ok(cut));
        }
    }
}

#[test]
fn only_a_checkout_has_a_path_and_only_an_address_a_url() {
    let address = RepoUrl::try_from("https://github.com/ada/roundhouse.git".to_owned()).unwrap();
    let origins = [
        RepoOrigin::Checkout { path: "/r".into() },
        RepoOrigin::Remote { url: address.clone() },
        RepoOrigin::Linked { path: "/r".into(), url: address.clone() },
        RepoOrigin::Unknown,
    ];

    let said: Vec<_> = origins.iter().map(|origin| (origin.checkout(), origin.url())).collect();

    assert_eq!(said, [(Some("/r"), None), (None, Some(&address)), (Some("/r"), Some(&address)), (None, None)]);
}

fn on_github(address: &str) -> Option<GithubRepositoryName> {
    RepoUrl::try_from(address.to_owned()).unwrap().on_github()
}

/// Every way git writes an address of github.com, with or without `.git`, a `/` at its end, a user or a port, whatever
/// the host's case.
#[test]
fn an_address_of_github_names_its_owner_and_name() {
    let cli = GithubRepositoryName::named("cli/cli");
    for address in [
        "https://github.com/cli/cli.git",
        "https://github.com/cli/cli",
        "https://github.com/cli/cli/",
        "HTTPS://GitHub.com/cli/cli.git",
        "http://github.com/cli/cli",
        "ssh://git@github.com/cli/cli.git",
        "ssh://git@github.com:22/cli/cli.git",
        "git+ssh://git@github.com/cli/cli",
        "git://github.com/cli/cli.git",
        "git@github.com:cli/cli.git",
        "git@github.com:/cli/cli.git",
        "github.com:cli/cli",
    ] {
        assert_eq!(on_github(address), Some(cli.clone()), "{address}");
    }
}

/// Another host, which the default isn't for, a path, and what has no owner and name.
#[test]
fn any_other_address_names_none() {
    for address in [
        "https://gitlab.com/cli/cli.git",
        "https://github.example.com/cli/cli.git",
        "https://notgithub.com/cli/cli.git",
        "git@gitlab.com:cli/cli.git",
        "/code/cli",
        "./cli:cli",
        "file:///code/cli.git",
        "https://github.com/cli",
        "https://github.com/cli/cli/pulls",
        "https://github.com/",
        "git@github.com:cli",
    ] {
        assert_eq!(on_github(address), None, "{address}");
    }
}
