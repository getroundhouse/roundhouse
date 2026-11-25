//! What a worker asks its hub, for a tool its agent called or a call it passes on, and the hub's answers. The hub knows
//! which workspace a worker is for, so an ask reaches that workspace, and the tool servers every worker has, and nothing
//! else.
//!
//! Each ask is a type of its own, which says what the hub answers it with, as [`Ask`] does, so a worker reads each
//! answer as that, and one of any other kind fails in one place, as a hub reads a worker's replies. On the wire, an
//! ask's fields sit beside its `method`, and an answer's beside its `result`.

mod comments;
mod pr_watch;
mod proposals;
mod pull_requests;
mod tool_servers;
mod worktrees;

pub use comments::{DiffComments, ReadDiffComments};
pub use proposals::{ProposalRecord, Propose, ReadProposal, Revise, Withdraw};

pub use pull_requests::{CancelWait, PullRequestAnswer, ReadPullRequest, ReadWait, WaitAnswer};
pub use tool_servers::RenewToolServer;
pub use worktrees::{
    CreateWorktree, ListWorktrees, ListedChild, NewWorktreeOn, OwnWorktree, RemoveWorktree, WorktreeCreated,
    WorktreeRemoval, WorktreeRemoved, Worktrees, WouldLose,
};

mod messages;
pub use messages::{Answer, Ask, AskAnswer, AskId, AskOutcome, AskReply, ToWorker, WorkerAsk};

pub use pr_watch::{PullRequestWaitUntil, ReadPullRequests, StartPullRequestWait};
