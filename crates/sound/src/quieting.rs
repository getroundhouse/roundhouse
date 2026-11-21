//! What's done to what's playing while something is heard, as the person chooses, so what's said isn't drowned out, or
//! heard with it: the sound output is muted, what the app the media keys control plays is paused, or both.

use super::media::{PAUSES_MEDIA, PausedMedia, media_restored, pause_media};
use super::muting::{MUTES_OUTPUT, MutedOutput, mute_output, output_restored};

/// One way of quieting what's playing while something is heard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quieting {
    /// Mutes the system's sound output, which silences everything but an output whose volume only the device sets,
    /// such as many displays' speakers.
    MuteOutput,
    /// Pauses what the app the keyboard's media keys control plays, such as music or a video, whatever it plays from,
    /// and plays it again after, if it's still that app and still paused. Nothing else pauses.
    PauseMedia,
}

/// What was done to quiet what's playing, in one way, put back as it was once this is dropped.
pub struct Quieted {
    how: Quieting,
    _until_dropped: Undone,
}

/// What puts back what was quieted, once it's dropped.
enum Undone {
    Output { _until_dropped: MutedOutput },
    Media { _until_dropped: PausedMedia },
}

impl Quieting {
    pub const ALL: [Self; 2] = [Self::MuteOutput, Self::PauseMedia];

    /// Whether this platform can quiet what's playing this way, which the app asks before offering it.
    pub const fn is_possible(self) -> bool {
        match self {
            Self::MuteOutput => MUTES_OUTPUT,
            Self::PauseMedia => PAUSES_MEDIA,
        }
    }

    /// Quiets what's playing this way, without waiting for it, until what this returns is dropped. `None` where it
    /// won't, since the thread that would couldn't start.
    pub fn quiet(self) -> Option<Quieted> {
        let undone = match self {
            Self::MuteOutput => Undone::Output { _until_dropped: mute_output()? },
            Self::PauseMedia => Undone::Media { _until_dropped: pause_media()? },
        };
        Some(Quieted { how: self, _until_dropped: undone })
    }
}

impl Quieted {
    /// The way it quieted what's playing.
    pub fn how(&self) -> Quieting {
        self.how
    }
}

/// Waits until everything quieted, and let go of, is put back: the sound output unmuted and what was
/// paused playing again. It's for the app to wait for as it quits, since the threads that put them back end with the
/// app. What's let go of after this is called isn't waited for.
pub fn restored() -> impl Future<Output = ()> + Send + 'static {
    let waits = [output_restored(), media_restored()];
    async move {
        for wait in waits {
            let _ = wait.recv_async().await;
        }
    }
}
