//! Which subagent a name names, by its ID, or by the call that started it.

use std::sync::Arc;

use protocol::model::{Subagent, SubagentId};

/// The one of `subagents` that `named` names: the one with that ID, or else the one the call it names started, as the
/// hub finds one by either. A subagent tab opened before its subagent's record came names it by that call.
pub fn subagent_named<'a>(subagents: &'a [Arc<Subagent>], named: &SubagentId) -> Option<&'a Arc<Subagent>> {
    let by_id = subagents.iter().find(|subagent| subagent.id() == *named);
    by_id.or_else(|| subagents.iter().find(|subagent| SubagentId::of_call(&subagent.call) == *named))
}
