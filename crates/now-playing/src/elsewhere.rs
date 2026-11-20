//! Everywhere but a Mac, such as Windows and Linux, which don't pause what's playing yet, so [`PAUSES`] is false and
//! the app doesn't offer to.

/// Whether this platform can pause what's playing while it's dictated to. These can't yet.
pub const PAUSES: bool = false;

/// Never made: nothing is paused here.
pub enum Paused {}

/// Pauses nothing, as [`PAUSES`] says, so what's playing goes on playing while it's dictated to.
pub fn pause() -> Option<Paused> {
    None
}

/// Never called, since nothing is paused here.
#[expect(clippy::needless_pass_by_value, reason = "every platform's takes its `Paused`, which is the pause it ends")]
pub fn play_again(paused: Paused) {
    match paused {}
}
