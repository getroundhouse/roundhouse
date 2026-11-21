//! Protocol 5, the version before this one, which a worker of an older build speaks until the hub replaces it with one
//! of its own, once its workspace is idle. Only the hub uses it, and it goes with the next break, as
//! `docs/principles/protocol.md` says.
//!
//! What the hub sends such a worker, and everything such a worker sends, is this protocol's, so until protocol 5 goes,
//! a change can't take away a form either reads or sends without keeping a way to read or send it. For example, a
//! worker of protocol 5 can't read a request for the setup log without the worktree's path, so
//! [`SetupLog`](super::SetupLog) still sends it.

/// Protocol 5, the version before [`crate::worker::WORKER_PROTOCOL_VERSION`].
pub const VERSION: u32 = 5;
