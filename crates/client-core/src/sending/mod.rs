//! Prompts a person writes, on their way to the hub. Each adds its files to the session, then waits its turn in its
//! chat's line before the hub is sent it, so a chat's prompts are recorded in the order they were written, though an
//! earlier one is still adding a large file. One that doesn't arrive is handed back whole, so nothing written is lost.

#[cfg(test)]
use crate::attachments::FileToSend;
#[cfg(test)]
use crate::client::{Client, ClientError, OpenSession};
#[cfg(test)]
use protocol::model::Seq;

#[cfg(test)]
mod tests;

mod delivery;
pub use delivery::{NotTaken, Taking, Written};
