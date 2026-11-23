//! Each workspace's lines added and removed against its base, in all, which the hub keeps as an observation at its own
//! level, so every client has them for every workspace, whether or not any shows it. Nothing keeps them up to date, so
//! the hub reads them from the workspace's worker when something says they may have changed, as
//! `docs/principles/serving.md` says, and they're as of that read until the next: when a turn ends in the workspace,
//! and when its worker says what its branches are, which it does as the hub takes it on and as they move. A worker
//! from before the request refuses it, which leaves them as they were until the hub replaces that worker with one of
//! its own build, whose first word of the branches reads them. Only a ready workspace's are read, so an archived one
//! keeps what was last read of it, as of then, until it's brought back, and a removed one's go with it.

mod reading;
mod reads;
#[cfg(test)]
pub(super) mod tests;

pub(super) use reads::Reads;
