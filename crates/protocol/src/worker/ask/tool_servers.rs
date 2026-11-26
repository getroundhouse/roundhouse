//! What a worker asks its hub about a tool server that refused a call it passed on.

use serde::{Deserialize, Serialize};

use super::super::{ProxiedServer, TokenNumber};
use super::{Answer, Ask, AskReply, WorkerAsk};
use crate::model::{ToolScopes, ToolServerId};

/// What to call the server `server` with, now that it has answered a call with its access token `refused` with a 401.
/// The hub renews its sign-in, and answers once that has ended. It renews only the token it has, so each of the workers
/// asking about one token, and one that asks twice, has the same answer from one renewal. Answered with the server as
/// the hub then has it: with another token, with the same one when there's no other to be had for now, or needing the
/// person to sign in.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct RenewToolServer {
    pub server: ToolServerId,
    pub refused: TokenNumber,
}

impl Ask for RenewToolServer {
    type Answer = ProxiedServer;

    fn into_ask(self) -> WorkerAsk {
        WorkerAsk::RenewToolServer(self)
    }
}

impl Answer for ProxiedServer {
    fn read(reply: AskReply) -> Result<Self, AskReply> {
        let AskReply::ToolServer(answer) = reply else { return Err(reply) };
        Ok(answer)
    }
}

/// Says that the server `server` answered a call with its access token `refused` with a 403, since that isn't allowed
/// `wants`, each of which the server named. The hub keeps them beside the sign-in, for signing in again to ask for, and
/// has the server's record say it needs more access. Asking twice changes nothing more.
///
/// The token is still good for the rest, so the worker goes on calling with it, and only the call that was refused fails.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AskForToolAccess {
    pub server: ToolServerId,
    pub refused: TokenNumber,
    /// The server's own text, which the hub checks the number and the length of.
    pub wants: ToolScopes,
}

/// What the hub makes of a server that wants more access, for what the agent is told.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolAccess {
    /// The server's record says it needs more access, and signing in to it again asks for it.
    SignInAgain,
    /// Nothing is kept, since signing in again would change nothing: its sign-in was asked for these already, or they're
    /// more than the hub keeps.
    NotAskedFor,
    /// Nothing is kept, since the hub has another token than the one that was refused, which may be allowed more.
    OtherToken,
    /// What a newer hub says.
    #[serde(other)]
    Unknown,
}

impl Ask for AskForToolAccess {
    type Answer = ToolAccess;

    fn into_ask(self) -> WorkerAsk {
        WorkerAsk::AskForToolAccess(self)
    }
}

impl Answer for ToolAccess {
    fn read(reply: AskReply) -> Result<Self, AskReply> {
        let AskReply::ToolAccess(answer) = reply else { return Err(reply) };
        Ok(answer)
    }
}
