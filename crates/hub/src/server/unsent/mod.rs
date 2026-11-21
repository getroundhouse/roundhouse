//! How much of what's written to a device's connection the kernel holds unsent. The hub keeps one frame of a watched
//! terminal queued for a client at a time, and lets the next go once that one is written, which means taken by the
//! kernel, not sent. Left to size its send buffer by itself, the kernel takes frame after frame while a slow link
//! sends them, and a reply written after them waits for them all: at 1 Mbit/s, under a flood, 1.4 s, against 0.45 s
//! with this, as #2129 measured.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub(super) use unix::hold_little_unsent;
#[cfg(all(unix, test))]
pub(super) use unix::{MOST_UNSENT, unsent_held};
#[cfg(windows)]
pub(super) use windows::hold_little_unsent;
