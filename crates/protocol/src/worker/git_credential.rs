//! The git credential a hub sets on a worker: what git and `gh` use for github.com in every process the worker starts,
//! so agents push and open pull requests as the hub's default GitHub account on any machine, rather than as whatever
//! login the machine has.

use serde::{Deserialize, Serialize};

use super::{Request, WorkerRequest};
use crate::model::{GithubToken, GithubUsername};

/// Sets the credential every process the worker starts in its workspace uses for github.com: its agents, its setup and
/// archive scripts, and its own `git` and `gh`, and answers once it's applied, so a process started after the answer
/// has it. `None` takes it away. The hub sends it as each link opens, after the hello and before anything else, and
/// again whenever the credential changes, as when it's renewed or another account is made the default.
///
/// A process started under one has a newer one, or its removal, from its next `git`, since the worker rewrites what
/// each one reads, so a renewal reaches a running agent, and one started under a credential that was taken away can't
/// push until it starts again, rather than going on as the machine's own login unseen. One started with none keeps
/// the machine's own until it starts again.
///
/// A worker applies it on its connection's own loop, as it handles a hello and a stop, before it reads the next frame,
/// so sets apply in the order the hub sent them, and a link's last set is applied before the worker serves the next
/// link.
///
/// A worker that can't apply one here, such as one with nowhere private to keep it or a git too old to read it,
/// answers `Unavailable` and keeps no credential, emptying what it had, so its processes use the machine's own login.
/// One older than this answers `Unsupported`, and goes on with the machine's own.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GitCredentialSet {
    #[serde(default)]
    pub credential: Option<GitCredential>,
}

impl Request for GitCredentialSet {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::GitCredentialSet(self)
    }
}

/// What git and `gh` use for github.com, made from a GitHub account's login: for a token, the token itself, and never a sign-in's refresh token or an app's key. Its
/// token's `Debug` says only that it's there, so a request that carries one can be logged whole.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GitCredential {
    /// Whose it is, as GitHub names the user, which git gives GitHub with the token, and `gh` says it's logged in as.
    pub username: GithubUsername,
    pub token: GithubToken,
    /// Who the commits that agents make are by, for an app's bot. `None` leaves the machine's own, as for a person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<GitAuthor>,
}

/// Who a commit is by, as git's `user.name` and `user.email` say.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GitAuthor {
    pub name: String,
    pub email: String,
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};

    use super::*;

    fn author() -> impl Strategy<Value = GitAuthor> {
        ("[a-z]{1,12}(\\[bot\\])?", "[a-z0-9+]{1,20}@users\\.noreply\\.github\\.com")
            .prop_map(|(name, email)| GitAuthor { name, email })
    }

    fn credential(token: String) -> impl Strategy<Value = GitCredential> {
        ("[a-z][a-z0-9-]{0,38}", proptest::option::of(author())).prop_map(move |(username, author)| GitCredential {
            username: username.into(),
            token: GithubToken::new(token.clone()),
            author,
        })
    }

    fn setting() -> impl Strategy<Value = (GitCredentialSet, String)> {
        let token = "(ghp_|gho_|ghu_|ghs_|github_pat_)[A-Za-z0-9_]{8,60}";
        token.prop_flat_map(|token| {
            let set =
                proptest::option::of(credential(token.clone())).prop_map(|credential| GitCredentialSet { credential });
            (set, Just(token))
        })
    }

    proptest! {
        #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

        #[test]
        fn a_credential_reads_back_as_it_was_set(setting in setting()) {
            let (set, _) = setting;
            let written = serde_json::to_string(&set.clone().into_request()).unwrap();

            prop_assert_eq!(serde_json::from_str::<WorkerRequest>(&written).unwrap(), set.into_request());
        }

        /// The request whole, and the credential alone, as a worker might log either.
        #[test]
        fn a_credential_never_prints_its_token(setting in setting()) {
            let (set, token) = setting;

            let (request, credential) = (format!("{:?}", set.clone().into_request()), format!("{:?}", set.credential));

            prop_assert!(!request.contains(&token), "{}", request);
            prop_assert!(!credential.contains(&token), "{}", credential);
        }
    }
}
