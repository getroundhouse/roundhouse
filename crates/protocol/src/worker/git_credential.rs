//! The git credential a hub sets on a worker: what git and `gh` use for github.com in every process the worker starts,
//! so agents push and open pull requests as the hub's default GitHub account on any machine, rather than as whatever
//! login the machine has.

use serde::{Deserialize, Serialize};

use super::{Request, WorkerRequest};
use crate::model::{GithubToken, GithubUsername};

/// Sets what every process the worker starts uses for github.com: its agents, whatever their kind, its setup and
/// archive scripts, and its own `git` and `gh`. Answered once it's applied, so a process started after the answer has
/// it. The hub sends it as each link opens, before anything else, and again whenever it changes, as when the default
/// account is renewed or another is chosen.
///
/// A worker applies it on its connection's own loop, as it handles a hello and a stop, before it reads the next frame,
/// so sets apply in the order the hub sent them, and a link's last applies before the worker serves the next link.
///
/// What a process started under one uses changes with each set after it, from its next `git` or `gh`, since the
/// worker rewrites the files it reads: a renewal reaches an agent that's running, and so does a credential that's
/// [`Blocked`](GithubCredential::Blocked). One started under [`MachineOwn`](GithubCredential::MachineOwn) is given
/// none of it, so it keeps the machine's own until it next starts.
///
/// A worker from before this answers `Unsupported`, and goes on with the machine's own. One that can't apply it, such
/// as one whose git is older than 2.31, the first to read `GIT_CONFIG_COUNT`, or that can't write its files, answers
/// `Unavailable`, and what it starts uses the machine's own.
///
/// What still goes round it: the person's own `url.<ssh>.insteadOf` or `pushInsteadOf` for GitHub, and a host alias
/// such as `git@github-work:`, which push over SSH with the machine's key; tools built on libgit2, Git LFS, and tools
/// that read `GITHUB_TOKEN`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GitCredentialSet {
    pub credential: GithubCredential,
}

impl Request for GitCredentialSet {
    type Reply = ();

    fn into_request(self) -> WorkerRequest {
        WorkerRequest::GitCredentialSet(self)
    }
}

/// What a worker's processes use for github.com, from the hub's default GitHub account.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GithubCredential {
    /// The default is the machine's own login. The worker writes nothing that changes what git or `gh` use, so what it
    /// starts from then on uses the machine's own.
    MachineOwn,
    /// Everything the worker starts uses this for github.com.
    Account(GitCredential),
    /// The default can't be used, as when it was disconnected or its token ended, so nothing the worker starts can push
    /// or call GitHub as anyone. Git's request for a credential fails, saying `reason`, which says what to do, such as
    /// renew the account in Settings.
    Blocked { reason: String },
    /// One from a newer hub, which a worker treats as [`Blocked`](GithubCredential::Blocked), saying the hub is newer
    /// than it, rather than push as anyone the hub didn't mean.
    #[serde(other)]
    Unknown,
}

/// A GitHub account's username and token, which git gives GitHub for github.com, whatever username a remote names,
/// and `gh` says it's logged in as. Its token's `Debug` says only that it's there, so a request that carries one can
/// be logged whole. For an installation of an app of your own, the username is `x-access-token`, as GitHub's docs give
/// git for an installation's token.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GitCredential {
    pub username: GithubUsername,
    pub token: GithubToken,
    /// Who commits are by, as the account the hub added, whichever way it was connected, so `git commit` works on a
    /// machine with no git config of its own, and commits say the account rather than whoever the machine's git does.
    /// A worker sets it as git's `user.name` and `user.email` for every process it starts, from their next `git`, over
    /// the repository's and the person's own, and takes them out again with a credential that has none. A worker from
    /// before it reads past it, so it pushes as the account, and commits as whoever the machine's git says. `None`
    /// from a hub from before it, and from one that gives an account none, as this one does an account it hasn't asked
    /// GitHub about, which commits as the machine's git says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<GitIdentity>,
}

/// Who a commit is by, as git's `user.name` and `user.email` say it: for an app's installation, its bot,
/// `ada-roundhouse[bot]`, with the address GitHub links to the bot, `41+ada-roundhouse[bot]@users.noreply.github.com`,
/// and for a token's or a sign-in's account, the name its user goes by on GitHub, or its username, and the email it
/// shows there, or the address GitHub gives it, such as `7+ada@users.noreply.github.com`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GitIdentity {
    pub name: String,
    pub email: String,
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngSeed};

    use super::*;

    fn credential(token: String) -> impl Strategy<Value = GithubCredential> {
        let identity = proptest::option::of(("[a-z][a-z0-9-]{0,30}", 1..u64::MAX).prop_map(|(slug, id)| GitIdentity {
            name: format!("{slug}[bot]"),
            email: format!("{id}+{slug}[bot]@users.noreply.github.com"),
        }));
        let account = ("[a-z][a-z0-9-]{0,38}", identity).prop_map(move |(username, identity)| {
            GithubCredential::Account(GitCredential {
                username: username.into(),
                token: GithubToken::new(&token),
                identity,
            })
        });
        let blocked = ".{0,40}".prop_map(|reason| GithubCredential::Blocked { reason });
        prop_oneof![Just(GithubCredential::MachineOwn), account, blocked]
    }

    /// A set and the token it may carry.
    fn setting() -> impl Strategy<Value = (GitCredentialSet, String)> {
        let token = "(ghp_|gho_|ghu_|ghs_|github_pat_)[A-Za-z0-9_]{8,60}";
        token.prop_flat_map(|token| {
            (credential(token.clone()).prop_map(|credential| GitCredentialSet { credential }), Just(token))
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
