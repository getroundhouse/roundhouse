//! Playing a scenario on an agent whose adapter speaks JSON-RPC to its CLI, which are Codex and Antigravity: live on
//! the real CLI, or replayed on a stand-in for it. What's the same for both is here, and what's each one's own is in
//! its binding, such as `codex::conformance`, as a [`JsonRpcAgent`].

mod agent;
mod lines;
mod live;
mod replay;

pub(crate) use agent::JsonRpcAgent;
pub use live::Options;
pub(crate) use live::live;
