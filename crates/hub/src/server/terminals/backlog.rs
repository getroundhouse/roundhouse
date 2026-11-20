//! How far behind a client can fall on a terminal before it's sent what the terminal shows instead of what it missed.

use protocol::terminal::HeldUpdates;

/// The most output a client can be behind by before it's sent what the terminal shows instead, which is past what a
/// full history of a build's output takes as a screen, at 200 columns, about 1 MiB. A guess, not measured, to check
/// against a terminal with a colour in every cell, whose screen can be far larger.
pub(super) const BEHIND_BYTES: usize = 4 * 1024 * 1024;

/// The most updates it holds, frames of output among them, so a client too far behind on small updates, such as
/// another client's new sizes, is sent what the terminal shows too.
pub(super) const BEHIND_UPDATES: usize = 1024;

/// Whether a client whose follower holds `held` for it is too far behind, past [`BEHIND_BYTES`] or [`BEHIND_UPDATES`].
pub(super) fn too_far_behind(held: &HeldUpdates) -> bool {
    held.output_bytes() > BEHIND_BYTES || held.len() > BEHIND_UPDATES
}
