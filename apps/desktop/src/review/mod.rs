//! A workspace's diff against its base, and the review comments the hub keeps on it. It draws nothing: the sidebar of
//! changes lists its files, and the file tab shows one of them, with its comments.

mod comments;

pub use comments::{comment_line, comments_prompt};

mod review_view;
pub use review_view::{Review, ReviewEvent};
