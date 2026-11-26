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
