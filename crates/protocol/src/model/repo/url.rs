use std::fmt;
use std::ops::Range;

use serde::{Deserialize, Serialize};

/// The address a repository lives at, as git names a remote's, such as `https://github.com/ada/roundhouse.git` or
/// `git@github.com:ada/roundhouse.git`. It never holds a credential, so it's kept, sent and logged as it is: what git
/// can carry in one, such as the token in `https://ada:<token>@github.com/ada/roundhouse.git`, is cut out by
/// [`RepoUrl::without_credential`], and refused everywhere else, including as it's read.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
#[serde(try_from = "String", into = "String")]
pub struct RepoUrl(String);

/// Why text can't be a repository's address. It never holds the text, since that can be a credential.
#[derive(thiserror::Error, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepoUrlError {
    #[error("a repository's address can't be empty")]
    Empty,
    #[error("a repository's address can't hold a credential")]
    HoldsCredential,
}

impl RepoUrl {
    /// What git names a remote with, as `url`, without the credential it may hold, as a worker reads one from a
    /// checkout. `None` for an empty one, which names nowhere.
    pub fn without_credential(url: &str) -> Option<Self> {
        let url = match credential(url) {
            Some(credential) => [&url[..credential.start], &url[credential.end..]].concat(),
            None => url.to_owned(),
        };
        (!url.is_empty()).then_some(Self(url))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RepoUrl {
    type Error = RepoUrlError;

    fn try_from(url: String) -> Result<Self, Self::Error> {
        if url.is_empty() {
            return Err(RepoUrlError::Empty);
        }
        if credential(&url).is_some() {
            return Err(RepoUrlError::HoldsCredential);
        }
        Ok(Self(url))
    }
}

impl From<RepoUrl> for String {
    fn from(url: RepoUrl) -> String {
        url.0
    }
}

impl fmt::Display for RepoUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where in `url` a credential is, to cut out: before the `@` of an address with a scheme, such as `https://`, which
/// git hands its credential helpers whole, and a token can be alone, as GitHub takes one. Over SSH it's only a
/// password, after the user's name and a `:`, since the name is who SSH logs in as, such as `git`. An address with no
/// scheme has none: git reads `git@github.com:ada/roundhouse.git` as SSH's, where the name is SSH's too, and has no
/// place for a password, and a path has no user.
fn credential(url: &str) -> Option<Range<usize>> {
    let (scheme, rest) = url.split_once("://")?;
    let start = scheme.len() + "://".len();
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    // The last, since a password may hold an `@` of its own.
    let at = start + authority.rfind('@')?;
    let ssh = ["ssh", "git+ssh", "ssh+git"].iter().any(|ssh| scheme.eq_ignore_ascii_case(ssh));
    if ssh {
        let password = start + url[start..at].find(':')?;
        Some(password..at)
    } else {
        Some(start..at + "@".len())
    }
}
