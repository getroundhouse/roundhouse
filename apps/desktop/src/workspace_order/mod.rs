//! The orders workspaces can be listed in. The sidebar and Home each offer some of them, in a list of their own, so a
//! new order is a variant here, with what it's called and how it sorts, and is offered wherever its list names it.

#[cfg(test)]
mod tests;

mod sorting;
pub(crate) use sorting::Sort;
