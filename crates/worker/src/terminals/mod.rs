//! The shells a workspace's terminals run, each in a PTY in the worktree. A thread of each terminal's own keeps what it
//! shows, from everything its program writes, and sends the hub what happens in it as it happens, as "Terminals" in
//! `docs/architecture.md` says.

mod running;
mod shell;

pub(crate) use running::Terminals;
