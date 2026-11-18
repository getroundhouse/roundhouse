//! The approval modes a chat of Claude's can be given, as its adapter claims them, for tests whose agent is Claude, and
//! what else of Claude's claims a client acts on.

use crate::capability::approvals::{ApprovalMode, Beyond, DecisionPart, Kept, Modes, Requests, Unasked};
use crate::capability::{Capabilities, context};

/// Full access, which asks about nothing but what Claude's CLI asks about in any mode.
pub fn full_access() -> ApprovalMode {
    ApprovalMode { unasked: Unasked::Everything, beyond: Beyond::Asks, sandbox: None, plan: false }
}

/// Plan mode, which reads unasked and asks about the rest until its plan is approved.
pub fn plan_mode() -> ApprovalMode {
    ApprovalMode { unasked: Unasked::Reads, plan: true, ..full_access() }
}

/// Both, as a kind's claims offer them: full access first, as a chat starts.
pub fn modes() -> Modes {
    Modes { default: full_access(), others: vec![plan_mode()] }
}

/// What Claude's kind claims, as far as a client acts on a kind's claims: its modes, a refusal in words, and compacting
/// when asked.
pub fn claude_claims() -> Capabilities {
    let mut claims = Capabilities::default();
    claims.approvals.modes = Some(modes());
    let parts = vec![DecisionPart::KeepAllow, DecisionPart::Words, DecisionPart::Stop];
    claims.approvals.requests =
        Some(Requests { parts, kept: vec![Kept::Conversation, Kept::Worktree, Kept::Project, Kept::User] });
    claims.context.compaction = Some(context::Compaction { keeping: context::Keeping::Read });
    claims
}
