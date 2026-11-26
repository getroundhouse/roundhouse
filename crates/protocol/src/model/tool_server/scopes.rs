use std::fmt;

use serde::{Deserialize, Serialize};

/// How many scopes are kept of what another machine says, and how long each can be: far more than a server was seen to
/// name, and few enough that one that names new ones at each call can't have its file and its sign-in's page grow with
/// each. *Unconfirmed.*
const MOST: usize = 32;
const LONGEST: usize = 200;

/// What a sign-in to a tool server allows, as OAuth names it, such as `read`, in the order it was said, each once. As
/// what a sign-in is asked to allow, empty leaves it to the authorization server. It's text on the wire and in a file,
/// with a space between each, as OAuth's `scope` parameter has them.
///
/// What isn't a scope is left out as they're read. Whoever keeps what another machine said checks that there's no more
/// of it than [`ToolScopes::are_few`] says, or reads it with [`ToolScopes::said`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub struct ToolScopes(Vec<String>);

impl ToolScopes {
    pub fn of<S: AsRef<str>>(said: impl IntoIterator<Item = S>) -> Self {
        let mut scopes = ToolScopes::default();
        for scope in said {
            scopes.add(scope.as_ref());
        }
        scopes
    }

    /// What another machine `said` in OAuth's `scope` parameter. `None` where it says none, where any of it isn't a
    /// scope, and where there's more of it than is kept.
    pub fn said(said: &str) -> Option<Self> {
        let mut scopes = ToolScopes::default();
        for scope in said.split(' ').filter(|scope| !scope.is_empty()) {
            scopes.add(scope);
            // As each is added, so a long text isn't gone through first.
            if !scopes.has(scope) || !scopes.are_few() {
                return None;
            }
        }
        (!scopes.is_empty()).then_some(scopes)
    }

    /// Whether there are few enough of these, and each is short enough, to keep of what another machine said.
    pub fn are_few(&self) -> bool {
        self.0.len() <= MOST && self.0.iter().all(|scope| scope.len() <= LONGEST)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }

    /// These, and then those of `others` that aren't among them. Signing in again asks for what was asked for and
    /// granted before as well, so nothing that worked stops working.
    #[must_use]
    pub fn and(mut self, others: &ToolScopes) -> Self {
        for scope in &others.0 {
            self.add(scope);
        }
        self
    }

    /// These, but for those that are among `others`.
    #[must_use]
    pub fn without(mut self, others: &ToolScopes) -> Self {
        self.0.retain(|scope| !others.has(scope));
        self
    }

    /// Whether each of `others` is among these.
    pub fn has_all(&self, others: &ToolScopes) -> bool {
        others.iter().all(|scope| self.has(scope))
    }

    pub fn has(&self, scope: &str) -> bool {
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

impl From<String> for ToolScopes {
    fn from(said: String) -> Self {
        ToolScopes::of(said.split(' '))
    }
}

impl From<ToolScopes> for String {
    fn from(scopes: ToolScopes) -> Self {
        scopes.to_string()
    }
}

/// As OAuth's `scope` parameter has them.
impl fmt::Display for ToolScopes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join(" "))
    }
}
