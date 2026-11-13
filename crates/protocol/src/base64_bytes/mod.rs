//! Bytes that travel as base64 text, since a frame is JSON, such as an attachment's or what a terminal's program wrote.

#[cfg(test)]
mod tests;

mod coding;
mod types;

pub(crate) use coding::{Base64Visitor, decode, encode};
pub(crate) use types::base64_bytes;
