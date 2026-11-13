//! Each workspace's other worktrees, made for its work. A record stays once its worktree is removed, with its branch,
//! so a worktree can be made on the branch again, and no two of a workspace's that aren't removed share a path.

#[cfg(test)]
mod tests;

mod worktree_rows;
