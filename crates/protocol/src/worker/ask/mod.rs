//! What a worker asks its hub, for a tool its agent called, and the hub's answers. The hub knows which workspace a
//! worker is for, so an ask reaches that workspace and nothing else.
//!
//! Each ask is a type of its own, which says what the hub answers it with, as [`Ask`] does, so a worker reads each
//! answer as that, and one of any other kind fails in one place, as a hub reads a worker's replies. On the wire, an
//! ask's fields sit beside its `method`, and an answer's beside its `result`.

mod pull_requests;
mod worktrees;

pub use pull_requests::{CancelWait, PullRequestAnswer, ReadPullRequest, ReadWait, StartWait, WaitAnswer};
pub use worktrees::{
    CreateWorktree, ListWorktrees, ListedChild, NewWorktreeOn, OwnWorktree, RemoveWorktree, WorktreeCreated,
    WorktreeRemoval, WorktreeRemoved, Worktrees, WouldLose,
};

mod messages;
pub use messages::{Answer, Ask, AskAnswer, AskId, AskOutcome, AskReply, ToWorker, WorkerAsk};
