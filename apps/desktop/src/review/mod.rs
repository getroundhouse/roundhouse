//! A workspace's diff against its base, and the review comments the hub keeps on it. It draws nothing: the sidebar of
//! changes lists its files, from the hub's summary of them where it sends one, and the file tab shows one of them,
//! with its comments.

mod comments;
mod listing;
pub use listing::{ChangedRow, Listing, Paging};

mod review_view;
pub use review_view::{Review, ReviewEvent};
