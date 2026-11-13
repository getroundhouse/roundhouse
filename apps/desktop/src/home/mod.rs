//! Home: every workspace in one list, shown in place of the workspace, to find one by its name, narrow it to a
//! repository or to what's archived, and sort.

mod bar;

#[cfg(test)]
use crate::workspace_order::Sort;

#[cfg(test)]
mod tests;

mod projects;

#[cfg(test)]
use projects::listed;
pub(crate) use projects::{ArchiveFilter, Home, Rows, SORTS, arrange, page};
