//! Comments left on a diff, and what saving or deleting one does to the comment being written there.

#[cfg(test)]
mod tests;

mod comment_rows;
pub use comment_rows::Commented;
pub(super) use comment_rows::{COMMENT_COLUMNS, comment_from_row};
