//! Each workspace's changes against its base, which the hub keeps as two observations from each summary its worker
//! sends: the lines added and removed, in all, at the hub's level, so every client has them for every workspace,
//! whether or not any shows it, and the files changed, at the workspace's own level, so a client that follows the
//! workspace has them. While a client follows the workspace and its worker is running, the worker watches its worktree
//! and sends the summary as it changes, as `super::changes_watch` says, and both are current. Otherwise the hub reads
//! them from the workspace's worker when something says they may have changed, as `docs/principles/serving.md` says,
//! and they're as of that read until the next: when a turn ends in the workspace, and when its worker says what its
//! branches are, which it does as the hub takes it on and as they move. A worker from before the request refuses it,
//! which leaves them as they were until the hub replaces that worker with one of its own build, whose first word of the
//! branches reads them. Only a ready workspace's are read. An archived one keeps its lines as last read, as of then,
//! until it's brought back, and forgets its files, and a removed one's both go with it.

mod kept;
mod reading;
mod reads;
#[cfg(test)]
pub(super) mod tests;

pub(super) use reads::Reads;
