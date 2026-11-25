//! Made-up records and events, for the tests of every crate. Each is valid from the fewest arguments, and a test sets
//! what it's about with struct update syntax, so a field added to a record changes its builder here rather than every
//! test that makes one. A block is an enum's variant, which that syntax can't set, so a block has a builder for each
//! shape tests make, such as [`of_kind`].

mod pr_wait;
mod proposal;
pub use pr_wait::{checked, pull_request, pull_request_wait};
pub use proposal::proposal;

mod references;
pub use references::issue;

mod repo;
pub use repo::repo;

mod typed;
pub use typed::{
    command_output, image, typed_call, typed_command, typed_denied, typed_request, typed_result, typed_subagent,
    with_images,
};

mod builders;
pub use builders::{
    ApprovalRequest, account, allow_once, approval_request, approve_plan, asking, cancelled, child_worktree,
    claude_default, decided, deny, event, github_account, github_app, github_sign_in, github_user, plan, question,
    request_failed, session, started_claiming, subagent, subagent_started, to_run, turn_ended, workspace,
};

mod tool_server;
pub use tool_server::tool_server;

pub mod observed;

mod files;
pub use files::{WEBM_START, webm};

mod decisions;
pub use decisions::passed_decisions;

mod modes;
pub use modes::{claude, claude_claims, claude_models, full_access, modes, offer, plan_mode};
