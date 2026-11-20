//! What Claude's adapter recorded before it typed its calls, typed as it records them now: each call by the tool's name
//! and input, what came back for each, each request for approval, and what a subagent's record said it had done lately,
//! and what the hub recorded of each decision before it typed them. Codex's and Antigravity's adapters typed their
//! requests from the start, beside fields of the older form, which go. Every adapter named its approval modes before
//! they were typed, as the hub did, which `modes` types by each chat's kind. The store's migration types what the hub
//! kept, and the worker link what a worker of protocol 4 still sends, since only one of a build from before can, so
//! nothing past them reads the older forms.
//!
//! It's frozen, as Claude's tools and words were when the adapter stopped recording them, and it never changes, while
//! the adapter's own table follows the CLI, so they aren't one. A result is read only from what the adapter kept of it,
//! since what the CLI said beside it is only in the event's raw line, which nothing reads again.
//!
//! *Unconfirmed:* typing what was recorded before in the hub, once, rather than in every client.

mod calls;
mod lately;
mod modes;
mod older;
mod outputs;
mod requests;
#[cfg(test)]
mod tests;
mod typing;

pub(crate) use lately::lately;
pub(crate) use modes::{mode, type_mode};
pub(crate) use requests::{type_request, type_resolution};
pub(crate) use typing::{Typing, naming_a_mode};
