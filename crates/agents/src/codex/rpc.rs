//! Codex app-server's bare JSON-RPC dialect and refusal wording.

use crate::AgentError;
pub(super) use crate::jsonrpc::{Dialect, REPLY_WAIT, Rpc, refused};

/// Whether Codex refused a request for a turn because it has none going, as it says once the turn has ended. Its words
/// are all that tells this from another refusal, such as of a turn that can't be steered, and they're 0.169.0's. If
/// they change, the refusal is taken as one for another reason, which is safe, but slower to recover from.
pub(super) fn no_active_turn(error: &AgentError) -> bool {
    matches!(error, AgentError::Refused(message) if message.starts_with("no active turn"))
}
