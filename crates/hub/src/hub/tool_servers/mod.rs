//! The tool servers the hub has of its own, as a client asks, and as the hub starts: checking them, adding them,
//! editing them and removing them, signing in to them, and renewing those sign-ins. What's kept, in what order, and
//! what's refused is `crate::tool_servers`'s, and what's asked of a server and its authorization server is the
//! `tool-sign-in` crate's.

#[cfg(test)]
mod tests;

mod check_calls;
mod in_doubt;
mod reconnecting;
mod renewals;
mod renewing;
mod save_calls;
mod saves;
mod server_calls;
mod sign_in_calls;
mod sign_ins;
mod waiting;
mod wanted;
pub(super) use sign_ins::ToolSignIns;
