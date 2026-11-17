use std::borrow::Cow;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::super::{GithubUsername, RepoId, Rev};

/// Whether agents can push as the default GitHub account to the repository on GitHub that a repository's `origin`
/// names, as the hub last checked, as a record of its own, keyed by the repository, so every workspace of it shows the
/// same. Whether a push is taken is the repository's and the account's to say, not a workspace's, so it's checked once
/// a repository and login: as the hub starts, as a worker of the repository first says which repository on GitHub it
/// pushes to, as the default or its login changes, and every half hour, since what an account can do changes on
/// GitHub too, as when an organization approves Roundhouse. It blocks nothing: agents push as they would anyway. A
/// repository no worker has said of since the hub first checked any has none. What GitHub answered is a cache of
/// GitHub's, so it says when GitHub answered, and one the hub's last run left says so until it's checked again.
/// *Unconfirmed:* a record of its own, the half hour, and `answered_at_ms` and `stale` as the shape.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct GithubReach {
    pub repo: RepoId,
    pub reach: Reach,
    /// When GitHub gave the answer `reach` is, by the hub's clock, in milliseconds since the Unix epoch. `None` for
    /// what isn't GitHub's answer, which the hub says without asking it, and from a hub from before it said.
    #[serde(default)]
    pub answered_at_ms: Option<i64>,
    /// The hub's last run left it, and this one hasn't checked it since it started, so it's as it was then. Checked
    /// again, it's this run's. `false` from a hub from before it said.
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub rev: Rev,
}

/// What a push from a repository's workspaces would meet, as the hub last checked, by GitHub's own answer to what
/// `git push` asks before it sends anything.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Reach {
    /// GitHub would take a push to `repository` as `username`, the default. A branch's own rules, such as one that's
    /// protected, are met only by the push itself, so a push to such a branch can still be refused.
    Pushes { repository: GithubRepositoryName, username: GithubUsername },
    /// GitHub refused a push to `repository` as `username`, saying why in `reason`, in its own words, as `git` prints
    /// them after `remote:`, such as `Permission to cli/cli.git denied to ada.`, or what an organization's SAML SSO or
    /// its restrictions on OAuth Apps ask for.
    Refused { repository: GithubRepositoryName, username: GithubUsername, reason: String },
    /// GitHub has no repository `repository` that `username` can see: it isn't there, or it's private and not theirs.
    NotFound { repository: GithubRepositoryName, username: GithubUsername },
    /// GitHub didn't take `username`'s token at all, as when it has been revoked on GitHub.
    TokenRefused { repository: GithubRepositoryName, username: GithubUsername },
    /// GitHub couldn't be asked, or didn't say, as `why` says, such as while it can't be reached.
    CouldntTell { repository: GithubRepositoryName, why: String },
    /// Not checked, since the default is each machine's own login, which the hub holds no token for, so a push goes as
    /// whatever that machine's git has.
    MachineOwn { repository: GithubRepositoryName },
    /// Not checked, since the default's login can't be used now, so a push to GitHub fails, saying `reason`, as the
    /// hub's block of every worker's git credential does.
    Blocked { repository: GithubRepositoryName, reason: String },
    /// The repository's `origin` isn't on github.com, or there's none, so the default isn't what it pushes with.
    NotOnGithub,
    /// From a newer hub.
    #[serde(other)]
    Unknown,
}

impl Reach {
    /// The repository on GitHub it's about, unless it's on none, or is one this build can't read.
    pub fn repository(&self) -> Option<&GithubRepositoryName> {
        match self {
            Reach::Pushes { repository, .. }
            | Reach::Refused { repository, .. }
            | Reach::NotFound { repository, .. }
            | Reach::TokenRefused { repository, .. }
            | Reach::CouldntTell { repository, .. }
            | Reach::MachineOwn { repository }
            | Reach::Blocked { repository, .. } => Some(repository),
            Reach::NotOnGithub | Reach::Unknown => None,
        }
    }

    /// The repository and the account GitHub gave this answer for, when it's GitHub's answer rather than what the hub
    /// says without asking it.
    pub fn answered_for(&self) -> Option<(&GithubRepositoryName, &GithubUsername)> {
        match self {
            Reach::Pushes { repository, username }
            | Reach::Refused { repository, username, .. }
            | Reach::NotFound { repository, username }
            | Reach::TokenRefused { repository, username } => Some((repository, username)),
            Reach::CouldntTell { .. }
            | Reach::MachineOwn { .. }
            | Reach::Blocked { .. }
            | Reach::NotOnGithub
            | Reach::Unknown => None,
        }
    }
}

/// A repository on github.com, by its owner and its name, as GitHub writes them in its address, such as `cli/cli`.
/// Each is GitHub's letters, digits, `-`, `_` and `.`, and neither is only dots, so it can go in an address as it is.
/// A plain `owner/name` on the wire.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(try_from = "String", into = "String")]
pub struct GithubRepositoryName(Cow<'static, str>);

/// Why text isn't a repository on GitHub's owner and name.
#[derive(thiserror::Error, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GithubRepositoryNameError {
    #[error("a repository on GitHub is its owner and its name, with one `/` between")]
    NotOwnerAndName,
    #[error("a repository on GitHub's owner and name are letters, digits, `-`, `_` and `.`, and neither is only dots")]
    NotGithubs,
}

impl GithubRepositoryName {
    /// A repository written into this build. It's checked as it's built, so a constant that names none doesn't compile.
    pub const fn named(name: &'static str) -> Self {
        assert!(check(name).is_ok(), "that names no repository on GitHub");
        Self(Cow::Borrowed(name))
    }

    /// `owner`'s repository `name`.
    pub fn new(owner: &str, name: &str) -> Result<Self, GithubRepositoryNameError> {
        Self::try_from(format!("{owner}/{name}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for GithubRepositoryName {
    type Error = GithubRepositoryNameError;

    fn try_from(written: String) -> Result<Self, Self::Error> {
        check(&written)?;
        Ok(Self(Cow::Owned(written)))
    }
}

impl From<GithubRepositoryName> for String {
    fn from(name: GithubRepositoryName) -> String {
        name.0.into_owned()
    }
}

impl fmt::Display for GithubRepositoryName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether `written` is an owner and a name, as [`GithubRepositoryName`] says. It's `const`, so
/// [`GithubRepositoryName::named`] can check a constant as it compiles, which is why it goes byte by byte.
const fn check(written: &str) -> Result<(), GithubRepositoryNameError> {
    let bytes = written.as_bytes();
    let (mut at, mut slashes, mut part_starts, mut only_dots) = (0, 0, 0, true);
    while at <= bytes.len() {
        if at == bytes.len() || bytes[at] == b'/' {
            if at == part_starts {
                return Err(GithubRepositoryNameError::NotOwnerAndName);
            }
            // `.` and `..` are no names, and would move an address up its path.
            if only_dots {
                return Err(GithubRepositoryNameError::NotGithubs);
            }
            if at < bytes.len() {
                slashes += 1;
            }
            part_starts = at + 1;
            only_dots = true;
        } else if bytes[at].is_ascii_alphanumeric() || matches!(bytes[at], b'-' | b'_' | b'.') {
            only_dots = only_dots && bytes[at] == b'.';
        } else {
            return Err(GithubRepositoryNameError::NotGithubs);
        }
        at += 1;
    }
    if slashes == 1 { Ok(()) } else { Err(GithubRepositoryNameError::NotOwnerAndName) }
}
