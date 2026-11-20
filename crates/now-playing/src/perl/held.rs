//! The one function perl calls, which holds a pause until `dictation` writes a byte to this process's standard input,
//! or the input ends.

use std::io::{self, Read as _};
use std::time::Duration;

use super::media_remote::{Command, MediaRemote};
use crate::now_playing::{NowPlaying, wait_until_paused};

/// How long a pause is given to show in what `MediaRemote` says, which a player does in a moment.
const PAUSE_SHOWS_WITHIN: Duration = Duration::from_secs(2);

/// How often `MediaRemote` is asked meanwhile, each asking taking a few milliseconds.
const ASKED_EVERY: Duration = Duration::from_millis(20);

/// Pauses the Now Playing app, if it's playing, and waits until the pause shows. Then, once a byte comes on standard
/// input, which is when `dictation` lets go of the pause, or the input ends, as it does however the app ends, plays it
/// again if it's still the Now Playing app and still paused. Returns at once where it pauses nothing.
#[unsafe(no_mangle)]
extern "C" fn roundhouse_pause_while_held() {
    let Some(media) = MediaRemote::load() else { return };
    let Some(paused) = media.now_playing().and_then(NowPlaying::to_pause) else { return };
    media.send(Command::Pause);
    wait_until_paused(paused, || media.now_playing(), ASKED_EVERY, PAUSE_SHOWS_WITHIN);
    // Returns on a byte, at the end of the input, or where it can't be read, which is an end too.
    let _ = io::stdin().read_exact(&mut [0; 1]);
    if media.now_playing().is_some_and(|now| now.plays_again(paused)) {
        media.send(Command::Play);
    }
}
