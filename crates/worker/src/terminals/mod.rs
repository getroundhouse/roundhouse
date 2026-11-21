//! The shells a workspace's terminals run, each in a PTY in the worktree. A thread of each terminal's own keeps what it
//! shows, from everything its program writes, and sends the hub pictures of it while the hub watches, and its end.

mod process_records;
mod running;
mod shell;

pub(crate) use running::Terminals;
