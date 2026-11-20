//! The shells a workspace's terminals run, each in a PTY in the worktree. A thread of each terminal's own keeps what it
//! shows, from everything its program writes, and sends the hub what happens in it as it happens.

mod process_records;
mod running;
mod shell;

pub(crate) use running::Terminals;
