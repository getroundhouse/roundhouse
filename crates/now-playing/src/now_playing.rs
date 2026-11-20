//! What `MediaRemote` says of the Now Playing app, and what pausing it and playing it again decide from that.

use std::time::{Duration, Instant};

/// The Now Playing app, as `MediaRemote` says it is at one moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct NowPlaying {
    /// `None` while no app is the Now Playing app.
    pub(crate) app: Option<App>,
    pub(crate) playing: bool,
}

/// An app, by the ID of its process, as `MediaRemote` names the Now Playing app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct App(i32);

impl App {
    /// The app `MediaRemote` names with `pid`, which is 0 when it names none.
    pub(crate) fn from_pid(pid: i32) -> Option<Self> {
        (pid > 0).then_some(Self(pid))
    }
}

impl NowPlaying {
    /// The app to pause, which is the Now Playing app while it plays.
    pub(crate) fn to_pause(self) -> Option<App> {
        self.app.filter(|_| self.playing)
    }

    /// Whether to play `paused` again, which is only while it's still the Now Playing app and isn't playing. So an app
    /// that has taken its place isn't started, and one played again meanwhile is left as it is.
    pub(crate) fn plays_again(self, paused: App) -> bool {
        self.app == Some(paused) && !self.playing
    }

    /// Whether `paused` still shows as playing, as the system goes on saying for a moment after the pause is sent.
    fn shows_playing(self, paused: App) -> bool {
        self.app == Some(paused) && self.playing
    }
}

/// Waits until `now` no longer says `paused` plays, asking every `every`, for no longer than `within`. The system says
/// an app has paused a moment after the pause is sent, so without this, a dictation let go of at once would find what
/// it paused still playing, and leave it paused. Returns at once where `now` can't say. An app still playing after
/// `within` is taken not to have paused, so there's nothing to play again.
#[expect(clippy::disallowed_methods, reason = "the pace the system is asked at while a pause takes")]
pub(crate) fn wait_until_paused(
    paused: App,
    mut now: impl FnMut() -> Option<NowPlaying>,
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

    use super::{App, NowPlaying, wait_until_paused};

    const MUSIC: App = App(501);
    const BROWSER: App = App(502);

    #[test]
    fn only_an_app_that_plays_is_paused() {
        let cases = [
            (NowPlaying { app: Some(MUSIC), playing: true }, Some(MUSIC)),
            (NowPlaying { app: Some(MUSIC), playing: false }, None),
            (NowPlaying { app: None, playing: false }, None),
        ];

        for (now, paused) in cases {
            assert_eq!(now.to_pause(), paused, "{now:?}");
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
            assert_eq!(now.plays_again(MUSIC), plays, "{now:?}");
        }
    }

    /// How many times `wait_until_paused` asks, given what the system says each time, and how long it's given.
    fn asked(says: &[Option<NowPlaying>], within: Duration) -> usize {
        let asked = Cell::new(0);
        let now = || {
            asked.set(asked.get() + 1);
            says.get(asked.get() - 1).copied().flatten()
        };

        wait_until_paused(MUSIC, now, Duration::ZERO, within);

        asked.get()
    }

    #[test]
    fn a_pause_is_waited_for_until_it_shows_and_no_longer() {
        let playing = Some(NowPlaying { app: Some(MUSIC), playing: true });
        let paused = Some(NowPlaying { app: Some(MUSIC), playing: false });
        let another = Some(NowPlaying { app: Some(BROWSER), playing: true });
        let long = Duration::from_secs(60);

        assert_eq!(asked(&[playing, playing, paused], long), 3, "until it shows paused");
        assert_eq!(asked(&[playing, another], long), 2, "until another app takes its place");
        assert_eq!(asked(&[playing, None], long), 2, "until the system can't say");
        assert_eq!(asked(&[playing, playing, playing], Duration::ZERO), 1, "no longer than it's given");
    }

    #[test]
    fn no_process_is_no_app() {
        assert_eq!(App::from_pid(0), None);
        assert_eq!(App::from_pid(501), Some(MUSIC));
    }
}
