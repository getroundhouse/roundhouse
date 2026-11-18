//! Made-up records and events, for the tests of every crate. Each is valid from the fewest arguments, and a test sets
//! what it's about with struct update syntax, so a field added to a record changes its builder here rather than every
//! test that makes one. A block is an enum's variant, which that syntax can't set, so a block has a builder for each
//! shape tests make, such as [`of_kind`].

mod pr_wait;
pub use pr_wait::{observed_pull_request, pull_request_wait};

mod repo;
pub use repo::repo;

mod typed;
pub use typed::{
    command_output, image, typed_call, typed_command, typed_denied, typed_request, typed_result, typed_subagent,
    with_images,
};

mod builders;
pub use builders::{
    ApprovalRequest, account, allow_once, approval_request, approve_plan, asking, cancelled, child_worktree, decided,
    deny, event, found_github_login, github_account, github_app, github_reach, github_sign_in, plan, question,
    request_failed, session, subagent, subagent_started, to_run, turn_ended, workspace,
};

mod decisions;
pub use decisions::passed_decisions;
