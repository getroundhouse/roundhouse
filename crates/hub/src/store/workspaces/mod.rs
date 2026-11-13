//! Workspaces.

#[cfg(test)]
mod tests;

mod workspace_rows;

pub use workspace_rows::{BranchesSet, Ended, Move};
pub(super) use workspace_rows::{WORKSPACE_COLUMNS, workspace_from_row, workspace_on};
