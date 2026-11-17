//! GitHub's own numbers for what it has, which never change, and tell apart what a name can't.

use std::fmt;

use serde::{Deserialize, Serialize};

/// GitHub's own number for a user, which tells two users apart where their names can't, as when one renames itself
/// and another takes the name. A plain number on the wire.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct GithubUserId(u64);

/// GitHub's own number for a GitHub App, which its key signs as. A plain number on the wire.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct GithubAppId(u64);

/// GitHub's own number for one installation of a GitHub App. Installing the app again where it was removed is another.
/// A plain number on the wire.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct GithubInstallationId(u64);

macro_rules! numbers {
    ($($name:ident),+) => {$(
        impl $name {
            pub const fn new(id: u64) -> Self {
                Self(id)
            }

            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    )+};
}
numbers!(GithubUserId, GithubAppId, GithubInstallationId);
