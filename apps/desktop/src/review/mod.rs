//! A workspace's diff against its base, and the review comments the hub keeps on it. It draws nothing: the sidebar of
//! changes lists its files, from the hub's summary of them, and the file tab shows one of them, with its comments.

mod comments;
mod listed;
mod listing;
mod one_file;
pub use listed::Diffed;
pub use listing::{ChangedEntry, ChangedRow, Listing};

mod review_view;
pub use review_view::{Review, ReviewEvent};
