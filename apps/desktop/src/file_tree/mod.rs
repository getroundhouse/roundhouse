//! Paths in their folders, as the sidebar of changes and All files show them: which rows they make, and how a row is
//! drawn. Each list keeps its rows, and lays them out again only when its files or its open folders change, since a big
//! repository has thousands.

mod row;
#[cfg(test)]
mod tests;

pub(crate) use row::{List, file_frame, folder_row, row_frame};

mod tree;
pub(crate) use tree::{Row, Tree, chain, split};
