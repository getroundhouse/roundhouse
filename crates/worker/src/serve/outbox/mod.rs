//! What agents say, on its way to the hub. It goes to the hub connected, if one is. For a hub that asked the worker to
//! keep running without it, each frame the hub records is also numbered, and kept until the hub says it has recorded
//! it, so the next hub to connect gets whatever the last one didn't. A delta is never kept, since it isn't recorded,
//! and the message it's part of comes whole once it's finished. What the deltas so far have written of each message
//! that isn't finished is, though, so a hub that connects partway through a message gets what it missed, whether it
//! went to the hub before or to none.

#[cfg(test)]
use protocol::worker::WorkerFrame;
#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use tokio::sync::mpsc;

#[cfg(test)]
mod tests;

mod writing;
pub(crate) use writing::Outbox;
