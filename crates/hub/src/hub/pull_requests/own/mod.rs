//! Each workspace's own pull request, read by the hub from GitHub: the one of the branch on GitHub its worktree
//! pushes to, as the checked-out branch's upstream names it in the worker's branch reports. A repository and a branch
//! name are unique on GitHub, where local names repeat across machines, so that branch says which pull request a
//! workspace's work goes to, with no link to judge and no worker to ask. It's a relation, not ownership: two workspaces
//! on one branch have the same pull request.

mod judging;
mod reading;
#[cfg(test)]
mod tests;
