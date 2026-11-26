use serde::{Deserialize, Serialize};

use super::ToolServerHeader;

crate::tokens!(
    /// The key a tool server knows the person by, which the server gave them, sent in a header of each request.
    ToolServerKey,
);

/// How a tool server knows the person, as its record says. It never holds what the server knows them by.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolServerAuth {
    /// By a key, sent in `header`.
    Key { header: ToolServerHeader },
    /// By a sign-in, which the hub keeps and renews.
    SignIn { status: ToolSignInStatus },
    /// It doesn't, as one that anyone who can reach it can use.
    Neither,
    /// From a newer peer, or a newer build's row.
    #[serde(other)]
    Unknown,
}

impl ToolServerAuth {
    /// Whether the person has to sign in to it before any agent can use it.
    pub fn needs_sign_in(&self) -> bool {
        match self {
            ToolServerAuth::SignIn { status } => match status {
                ToolSignInStatus::NeedsSignIn => true,
                ToolSignInStatus::Works | ToolSignInStatus::Unknown => false,
            },
            ToolServerAuth::Key { .. } | ToolServerAuth::Neither | ToolServerAuth::Unknown => false,
        }
    }
}

/// Whether a tool server's sign-in works, as far as its authorization server has said.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolSignInStatus {
    /// It does, though its tokens can have ended while its authorization server can't be reached.
    Works,
    /// It's gone, so the person has to sign in again: its authorization server refused to renew it, or it has ended and
    /// nothing renews it.
    NeedsSignIn,
    /// From a newer peer, or a newer build's row.
    #[serde(other)]
    Unknown,
}

/// How a tool server is to know the person, as a client gives it to the hub, with what the server knows them by.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolServerAuthGiven {
    /// Both are as the person typed them, for the hub to check. The key is all the header is to hold, its scheme
    /// included, though the hub gives one in `Authorization` with no space in it, so with no scheme, `Bearer`.
    Key {
        header: String,
        key: ToolServerKey,
    },
    Neither,
    /// From a newer client, which the hub refuses.
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_sign_in_thats_gone_needs_the_person() {
        let signed_in = |status| ToolServerAuth::SignIn { status }.needs_sign_in();

        assert!(signed_in(ToolSignInStatus::NeedsSignIn));
        assert!(!signed_in(ToolSignInStatus::Works));
        assert!(!signed_in(ToolSignInStatus::Unknown), "a newer hub's");
        assert!(!ToolServerAuth::Neither.needs_sign_in());
        assert!(!ToolServerAuth::Unknown.needs_sign_in());
    }
}
