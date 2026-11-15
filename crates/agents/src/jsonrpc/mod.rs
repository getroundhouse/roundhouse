//! Newline-delimited JSON-RPC framing over child stdio, with independent client and server request IDs.

mod client;
#[cfg(test)]
mod tests;

pub(crate) use client::{Dialect, REPLY_WAIT, Rpc, RpcReply, refused};
