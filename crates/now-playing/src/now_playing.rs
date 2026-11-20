//! What the system says of the app the keyboard's media keys control, and what pausing it and playing it again decide
//! from that, on every platform that pauses, by whatever names an app there: a Mac's Now Playing app by its process,
//! and Windows' current media session by its app.

use std::time::{Duration, Instant};

/// How long a pause is given to show in what the system says, which a player does in a moment.
#[cfg(any(target_os = "macos", windows))]
pub(crate) const PAUSE_SHOWS_WITHIN: Duration = Duration::from_secs(2);

/// How often the system is asked meanwhile.
#[cfg(any(target_os = "macos", windows))]
pub(crate) const ASKED_EVERY: Duration = Duration::from_millis(20);

/// The app the media keys control, as the system says it is at one moment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NowPlaying<A> {
    /// `None` while no app is.
    pub(crate) app: Option<A>,
    pub(crate) playing: bool,
}

impl<A: PartialEq> NowPlaying<A> {
    /// The app to pause, which is the app while it plays.
    pub(crate) fn app_to_pause(self) -> Option<A> {
        self.app.filter(|_| self.playing)
    }

    /// Whether to play `paused` again, which is only while it's still the app the media keys control and isn't
    /// playing. So an app that has taken its place isn't started, and one played again meanwhile is left as it is.
    pub(crate) fn plays_again(&self, paused: &A) -> bool {
        self.app.as_ref() == Some(paused) && !self.playing
    }

    /// Whether `paused` still shows as playing, as the system goes on saying for a moment after the pause is sent.
    fn shows_playing(&self, paused: &A) -> bool {
        self.app.as_ref() == Some(paused) && self.playing
    }
}

/// Waits until `now` no longer says `paused` plays, asking every `every`, for no longer than `within`, which a platform
/// that pauses gives as `ASKED_EVERY` and `PAUSE_SHOWS_WITHIN`. The system says an app has paused a moment after the
/// pause is sent, so without this, a dictation let go of at once would find what it paused still playing, and leave it
/// paused. Returns at once where `now` can't say. An app still playing after `within` is taken not to have paused, so
/// there's nothing to play again.
#[expect(clippy::disallowed_methods, reason = "the pace the system is asked at while a pause takes")]
pub(crate) fn wait_until_paused<A: PartialEq>(
    paused: &A,
    mut now: impl FnMut() -> Option<NowPlaying<A>>,
    every: Duration,
    within: Duration,
) {
    let until = Instant::now() + within;
    while now().is_some_and(|now| now.shows_playing(paused)) && Instant::now() < until {
        std::thread::sleep(every);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::time::Duration;

    use super::{NowPlaying, wait_until_paused};

    const MUSIC: &str = "music";
    const BROWSER: &str = "browser";

    #[test]
    fn only_an_app_that_plays_is_paused() {
        let cases = [
            (NowPlaying { app: Some(MUSIC), playing: true }, Some(MUSIC)),
            (NowPlaying { app: Some(MUSIC), playing: false }, None),
            (NowPlaying { app: None, playing: false }, None),
        ];

        for (now, paused) in cases {
            assert_eq!(now.clone().app_to_pause(), paused, "{now:?}");
        }
    }

    #[test]
    fn only_the_paused_app_still_paused_plays_again() {
        let cases = [
            (NowPlaying { app: Some(MUSIC), playing: false }, true),
            (NowPlaying { app: Some(MUSIC), playing: true }, false),
            (NowPlaying { app: Some(BROWSER), playing: false }, false),
            (NowPlaying { app: None, playing: false }, false),
        ];

        for (now, plays) in cases {
            assert_eq!(now.plays_again(&MUSIC), plays, "{now:?}");
        }
    }

    /// How many times `wait_until_paused` asks, given what the system says each time, and how long it's given.
    fn asked(says: &[Option<NowPlaying<&'static str>>], within: Duration) -> usize {
        let asked = Cell::new(0);
        let now = || {
            asked.set(asked.get() + 1);
            says.get(asked.get() - 1).cloned().flatten()
        };

        wait_until_paused(&MUSIC, now, Duration::ZERO, within);

        asked.get()
    }

    #[test]
    fn a_pause_is_waited_for_until_it_shows_and_no_longer() {
        let playing = Some(NowPlaying { app: Some(MUSIC), playing: true });
        let paused = Some(NowPlaying { app: Some(MUSIC), playing: false });
        let another = Some(NowPlaying { app: Some(BROWSER), playing: true });
        let long = Duration::from_secs(60);

        assert_eq!(asked(&[playing.clone(), playing.clone(), paused], long), 3, "until it shows paused");
        assert_eq!(asked(&[playing.clone(), another], long), 2, "until another app takes its place");
        assert_eq!(asked(&[playing.clone(), None], long), 2, "until the system can't say");
        assert_eq!(asked(&[playing.clone(), playing.clone(), playing], Duration::ZERO), 1, "no longer than it's given");
    }
}
