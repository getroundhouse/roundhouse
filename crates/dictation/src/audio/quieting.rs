//! What a recording does to what's playing while it's dictated to, as the person chooses, so what's said isn't drowned
//! out, or heard with it: it mutes the sound output, pauses what the app the media keys control plays, or both.

use super::media::{PAUSES_MEDIA, PausedMedia, media_restored, pause_media};
use super::output::{MUTES_OUTPUT, MutedOutput, mute_output, output_restored};

/// One way a recording quiets what's playing while it's dictated to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quieting {
    /// Mutes the system's sound output, which silences everything but an output whose volume only the device sets,
    /// such as many displays' speakers.
    MuteOutput,
    /// Pauses what the app the keyboard's media keys control plays, such as music or a video, whatever it plays from,
    /// and plays it again after, if it's still that app and still paused. Nothing else pauses.
    PauseMedia,
}

/// What a recording did to quiet what's playing, in one way, put back as it was once this is dropped.
pub(crate) enum Quieted {
    Output {
        _until_dropped: MutedOutput,
    },
    Media {
        _until_dropped: PausedMedia,
    },
    /// Nothing, since the recording hears only what a test feeds it, which [`crate::Recording::quiets`] tells the test.
    #[cfg(any(test, feature = "testing"))]
    Fed(Quieting),
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
    pub(crate) fn quiet(self) -> Option<Quieted> {
        match self {
            Self::MuteOutput => mute_output().map(|muted| Quieted::Output { _until_dropped: muted }),
            Self::PauseMedia => pause_media().map(|paused| Quieted::Media { _until_dropped: paused }),
        }
    }
}

impl Quieted {
    pub(crate) fn how(&self) -> Quieting {
        match self {
            Self::Output { .. } => Quieting::MuteOutput,
            Self::Media { .. } => Quieting::PauseMedia,
            #[cfg(any(test, feature = "testing"))]
            Self::Fed(how) => *how,
        }
    }
}

/// Waits until everything recordings quieted, and have let go of, is put back: the sound output unmuted and what was
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
