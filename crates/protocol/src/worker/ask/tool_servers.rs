//! What a worker asks its hub about a tool server that refused a call it passed on.

use serde::{Deserialize, Serialize};

use super::super::{ProxiedServer, TokenNumber};
use super::{Answer, Ask, AskReply, WorkerAsk};
use crate::model::ToolServerId;

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
