//! What a hub sends a worker of protocol 3, the version before this one, which a worker of an older build still speaks
//! until the hub replaces it with one of its own, once its workspace is idle. Only the hub uses it, and it goes with the
//! next break, as "Changing the protocol" in `docs/architecture.md` says. A worker of protocol 4 reads each of these as
//! a request it doesn't know, and refuses it.
//!
//! Each keeps protocol 3's wire form, which `crates/protocol/tests/compatibility/` spells out, so a change to a type
//! they're made of that changes that form fails there, rather than each being a frozen copy of those types.
//!
//! What a worker of protocol 3 sends the hub is read with this protocol's types, so until protocol 3 goes, a change
//! can't take away a form such a worker sends, such as `AgentInit`'s claims, without keeping a way to read it.

mod launch_identity;
mod requests;

pub use requests::{AgentCancel, AgentCompact, AgentConfigure, AgentPrompt, AgentStart, Call, Request, VERSION};
