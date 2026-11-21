//! What the app the keyboard's media keys control plays, which is paused while something is heard, if the person has
//! chosen it, and played again once it stops, through `now-playing`, which does it each platform's way. Pausing waits
//! on another process, so it's done on a thread of its own, as `changes` makes each change. So the
//! next pause waits until what the one before paused is playing again, and doesn't take it for something to leave
//! paused.

use crate::changes::{Changes, Held};

/// Whether this platform can pause what's playing while something is heard.
pub(crate) const PAUSES_MEDIA: bool = now_playing::PAUSES;

/// What was playing, paused until this is dropped, if it was playing.
pub(crate) type PausedMedia = Held;

static PAUSING: Changes<now_playing::Paused> = Changes::new("sound-media", now_playing::pause, now_playing::play_again);

/// Has what's playing paused, if it's playing, without waiting for it. `None` where the thread couldn't start.
pub(crate) fn pause_media() -> Option<PausedMedia> {
    PAUSING.hold()
}

/// Answers once everything paused before it, and let go of, is playing again, as [`Changes::restored`] says.
pub(crate) fn media_restored() -> flume::Receiver<()> {
    PAUSING.restored()
}
