//! The hub's tool servers, which each agent reaches through its worker rather than at their own URLs, so the key each
//! takes is the worker's to send and no agent has it. What an agent sends is passed on as it is, and so is the answer,
//! as it comes. A server's 401 or 403 isn't passed on, since a CLI takes the server for failed and says only that its header
//! was rejected, which is the session's: the agent is told in words it can read. A server that's signed in to, and answers
//! 401, is asked once more, with the access token the hub has for it then, before the agent is told anything. One that
//! answers 403 since it wants more access fails that call alone, once the hub has been told what it wants. Each
//! request goes where the hub last set its server, so once the person moves one to another URL, an agent's next
//! request goes there, with whatever session it carries. A request that carries a sign-in's access token goes
//! over plain `http` only to a private address, as `upstream` says.

#[cfg(test)]
mod tests;

mod failure;
mod forwarding;
mod headers;
mod message;
mod passed_on;
mod path;
mod server;
mod under_way;
mod upstream;

use failure::{Failure, Sent};
pub(super) use forwarding::{Asked, METHODS};
pub(super) use passed_on::PassedOn;
pub(super) use path::server_in;
pub(crate) use server::Proxy;
use server::Target;
use under_way::UnderWay;
use upstream::Upstream;
