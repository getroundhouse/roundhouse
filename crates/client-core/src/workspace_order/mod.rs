//! The orders workspaces can be listed in, the same in every client. The desktop's sidebar and Home each offer some of
//! them, in a list of their own, so a new order is a variant here, with what it's called and how it sorts, and is
//! offered wherever its list names it. The phone lists each repository's workspaces in the one the sidebar starts in.

#[cfg(test)]
mod tests;

mod sorting;
pub use sorting::Sort;
