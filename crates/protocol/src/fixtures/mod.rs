//! Made-up records and events, for the tests of every crate. Each is valid from the fewest arguments, and a test sets
//! what it's about with struct update syntax, so a field added to a record changes its builder here rather than every
//! test that makes one. A block is an enum's variant, which that syntax can't set, so a block has a builder for each
//! shape tests make, such as [`of_kind`].

mod pr_wait;
pub use pr_wait::{observed_pull_request, pull_request_wait};

mod builders;
pub use builders::{
    ApprovalRequest, account, approval_request, call, call_result, child_worktree, command_failed, event, of_kind,
    session, subagent, tool_denied, tool_result, tool_use, turn_ended, workspace,
};
