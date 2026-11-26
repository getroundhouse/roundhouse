use std::error::Error;
use std::fmt;

use protocol::client::{ApiError, ErrorCode, error_chain};

use crate::NotFetched;

/// Why a call came to nothing.
#[derive(Debug, thiserror::Error)]
pub enum Failure {
    /// It couldn't be reached, or stopped part way, which may pass.
    #[error("couldn't reach {host}")]
    Unanswered {
        host: String,
        #[source]
        source: Box<dyn Error + Send + Sync>,
    },
    /// It didn't answer in time, which may pass.
    #[error("{host} didn't answer in time")]
    TimedOut { host: String },
    /// It answered with a 429 or a 5xx, which may pass.
    #[error("{host} answered {status}")]
    Busy { host: String, status: u16 },
    /// It answered with a status that gives neither what was asked for nor a reason OAuth has.
    #[error("{host} answered {status}")]
    Answered { host: String, status: u16 },
    /// Its answer isn't what a sign-in needs.
    #[error("{host} answered with {what}")]
    Unreadable { host: String, what: &'static str },
    /// It's at an address Roundhouse doesn't fetch.
    #[error("Roundhouse doesn't fetch from {host}, since {why}")]
    NotFetched { host: String, why: NotFetched },
    /// What the authorization server says of itself is said of another, at `host`, where what it said is an address.
    #[error("the authorization server {asked} says it's {}", .host.as_deref().unwrap_or("another"))]
    OtherIssuer { asked: String, host: Option<String> },
    /// What came back from the browser isn't of this sign-in, which is still waiting.
    #[error("what came back isn't of this sign-in")]
    WrongState,
    /// What came back from the browser isn't from the authorization server that was asked.
    #[error("what came back isn't from the authorization server that was asked")]
    WrongIssuer,
    /// The person refused it on the authorization server's page.
    #[error("the sign-in was refused")]
    RefusedByPerson,
    /// The authorization server refused the sign-in at its page.
    #[error("the authorization server refused the sign-in: {0}")]
    RefusedAtPage(OauthError),
    /// The authorization server gave no tokens for the code, or, of a renewal, said `invalid_client` of the client
    /// document, which may pass.
    #[error("the authorization server gave no tokens: {0}")]
    NoTokens(OauthError),
    /// The system gave no randomness to make a sign-in's secrets from.
    #[error("the system gave no randomness")]
    NoRandomness,
}

/// The most shown of an address that a server says is its own, longer than any is.
const LONGEST_SHOWN: usize = 200;

/// `said`, as a person is told it, such as an address that a server says is its own, cut short.
pub(crate) fn shown(said: &str) -> String {
    said.chars().take(LONGEST_SHOWN).collect()
}

/// The host of the address a server `said`, with its port where it names one, as a person is told it. `None` where
/// what it said isn't an address. A server's own text is never shown as Roundhouse's words, so only what an address
/// is read as is: a host, which can't hold a sentence, and is written in ASCII.
pub(crate) fn host_of(said: &str) -> Option<String> {
    let url = url::Url::parse(said).ok()?;
    let host = url.host_str().filter(|host| !host.is_empty())?;
    let host = url.port().map_or_else(|| host.to_string(), |port| format!("{host}:{port}"));
    Some(shown(&host))
}

/// What an authorization server said was wrong, as OAuth names it, such as `invalid_grant`. Its description is left
/// out, since it's the server's own text, which could have a token in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OauthError(String);

/// What OAuth calls a client the authorization server doesn't know, by RFC 6749, section 5.2.
const INVALID_CLIENT: &str = "invalid_client";

/// Longer than any name OAuth has for an error.
const LONGEST: usize = 64;

impl OauthError {
    /// What's `said`, where it could be one of OAuth's.
    pub fn new(said: &str) -> Self {
        let named = !said.is_empty() && said.len() <= LONGEST;
        let named = named && said.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
        Self(if named { said } else { "an error it didn't name" }.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether it's said of a client the authorization server doesn't know, such as one whose registration it has
    /// forgotten.
    pub fn is_invalid_client(&self) -> bool {
        self.0 == INVALID_CLIENT
    }
}

impl Failure {
    /// Whether it's that the authorization server doesn't know the client, as [`OauthError::is_invalid_client`] says.
    pub fn is_invalid_client(&self) -> bool {
        match self {
            Failure::NoTokens(error) | Failure::RefusedAtPage(error) => error.is_invalid_client(),
            Failure::Unanswered { .. }
            | Failure::TimedOut { .. }
            | Failure::Busy { .. }
            | Failure::Answered { .. }
            | Failure::Unreadable { .. }
            | Failure::NotFetched { .. }
            | Failure::OtherIssuer { .. }
            | Failure::WrongState
            | Failure::WrongIssuer
            | Failure::RefusedByPerson
            | Failure::NoRandomness => false,
        }
    }
}

impl fmt::Display for OauthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<Failure> for ApiError {
    fn from(failure: Failure) -> Self {
        let code = match &failure {
            Failure::Unanswered { .. } | Failure::TimedOut { .. } | Failure::Busy { .. } => ErrorCode::Unavailable,
            Failure::Answered { .. }
            | Failure::Unreadable { .. }
            | Failure::NotFetched { .. }
            | Failure::OtherIssuer { .. }
            | Failure::WrongState
            | Failure::WrongIssuer
            | Failure::RefusedByPerson
            | Failure::RefusedAtPage(_)
            | Failure::NoTokens(_) => ErrorCode::InvalidRequest,
            Failure::NoRandomness => ErrorCode::Internal,
        };
        ApiError::new(code, error_chain(&failure))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whatever a server says, only an address's host is shown of it, so no sentence of its own is.
    #[test]
    fn only_the_host_of_what_a_server_says_is_shown_and_nothing_of_what_isnt_an_address() {
        let cases = [
            ("https://other.example/mcp?a=b#c", Some("other.example")),
            ("https://Other.Example:8443/", Some("other.example:8443")),
            ("https://other.example:443/", Some("other.example")),
            ("https://bücher.example/", Some("xn--bcher-kva.example")),
            ("http://user:secret@10.0.0.1/", Some("10.0.0.1")),
            ("Sign in at evil.example instead", None),
            ("mailto:someone@other.example", None),
            ("", None),
        ];
        for (said, host) in cases {
            assert_eq!(host_of(said).as_deref(), host, "{said}");
        }
    }
}
