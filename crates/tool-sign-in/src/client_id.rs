use serde::{Deserialize, Serialize};

/// What an authorization server knows a client as: the address of its document, or what it was given as it registered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClientId(String);

/// Longer than one that holds all it says of its client, as those of a service that keeps no registrations do. It's sent
/// in an address, which some browsers and servers take no more of. *Unconfirmed.*
const LONGEST: usize = 2048;

impl ClientId {
    /// The client whose document is at `address`.
    pub(crate) fn of_document(address: impl Into<String>) -> Self {
        Self(address.into())
    }

    /// The one an authorization server gave, where it can be one, which RFC 6749, appendix A.1, has be printable ASCII.
    pub(crate) fn given(text: &str) -> Option<Self> {
        let printable = text.bytes().all(|b| b == b' ' || b.is_ascii_graphic());
        let is_one = printable && !text.trim().is_empty() && text.len() <= LONGEST;
        is_one.then(|| Self(text.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}
