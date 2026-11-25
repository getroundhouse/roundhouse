use std::fmt;

use serde::{Deserialize, Serialize};

/// The scope that asks for a refresh token, as OIDC names it.
const OFFLINE_ACCESS: &str = "offline_access";

/// What a sign-in is asked to allow, in the order it was said, each once. Empty leaves it to the authorization server.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub struct Scopes(Vec<String>);

impl Scopes {
    pub(crate) fn of<S: AsRef<str>>(said: impl IntoIterator<Item = S>) -> Self {
        let mut scopes = Scopes::default();
        for scope in said {
            scopes.add(scope.as_ref());
        }
        scopes
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// These, and then those of `others` that aren't among them. Signing in again asks for what was asked for before
    /// as well, so nothing that worked stops working.
    #[must_use]
    pub fn and(mut self, others: &Scopes) -> Self {
        for scope in &others.0 {
            self.add(scope);
        }
        self
    }

    /// These, and the one that asks for a refresh token. Empty stays empty, since asking for that one alone would ask
    /// for nothing of what the authorization server gives when it's left to it.
    #[must_use]
    pub(crate) fn offline(mut self) -> Self {
        if !self.is_empty() {
            self.add(OFFLINE_ACCESS);
        }
        self
    }

    pub(crate) fn has_offline(&self) -> bool {
        self.has(OFFLINE_ACCESS)
    }

    fn has(&self, scope: &str) -> bool {
        self.0.iter().any(|had| had == scope)
    }

    /// Leaves out what isn't a scope by RFC 6749, section 3.3, which has no spaces, quotes or backslashes.
    fn add(&mut self, scope: &str) {
        let said = !scope.is_empty() && scope.bytes().all(|b| matches!(b, 0x21 | 0x23..=0x5b | 0x5d..=0x7e));
        if said && !self.has(scope) {
            self.0.push(scope.to_string());
        }
    }
}

impl From<String> for Scopes {
    fn from(said: String) -> Self {
        Scopes::of(said.split(' '))
    }
}

impl From<Scopes> for String {
    fn from(scopes: Scopes) -> Self {
        scopes.to_string()
    }
}

/// As OAuth's `scope` parameter has them.
impl fmt::Display for Scopes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join(" "))
    }
}
