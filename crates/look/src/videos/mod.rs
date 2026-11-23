//! Videos: a picture of one, with a bar under it to play it, see where it is and go elsewhere in it, as the chat shows
//! one an agent's text points to and a file's tab shows one opened from the worktree. `docs/spec/components.md` says
//! how it looks.

#[cfg(test)]
mod tests;

mod length;
mod video;
pub use length::length;
pub use video::{VIDEO_LEAST_WIDTH, Video, VideoShows, video, video_bar_height};
