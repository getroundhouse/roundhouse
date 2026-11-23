//! A workspace's worker watching its worktree's files while a client follows the workspace, so what the hub keeps of
//! its changes is current, rather than as of the last turn's end. The hub's list of followers is what starts and stops
//! it: the hub tells the worker to watch once a subscription follows the workspace, and to stop once none does, as
//! `docs/principles/serving.md` says, and the worker sends the change summary whenever it differs. The hub doesn't
//! start a worker so that it can watch, as the owner decided: it tells one to watch once something else has started it,
//! as a prompt or a diff asked for does, and the paths that take a worker on tell it then. Only a ready workspace's
//! worker watches, so it's told again as archiving a workspace starts, and as every move ends. Until then, and under a
//! worker from before the request, the changes are as of their last read.

mod telling;
#[cfg(test)]
mod tests;
mod watches;

pub(super) use watches::Watches;
