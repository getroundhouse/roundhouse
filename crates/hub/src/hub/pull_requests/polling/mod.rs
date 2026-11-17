//! Background reads: choose the workspaces whose pull requests are due, and start a job for each token they're read as,
//! which reads them all together, while GitHub's count for that token leaves more than the last fifth.

mod scheduler;
#[cfg(test)]
mod tests;
pub(super) use scheduler::no_login;
