//! The one function perl calls, which holds a pause until `dictation` writes a byte to this process's standard input,
//! or the input ends.

use std::io::{self, Read as _};

use super::media_remote::{Command, MediaRemote};
use crate::now_playing::{ASKED_EVERY, NowPlaying, PAUSE_SHOWS_WITHIN, wait_until_paused};

/// Pauses the Now Playing app, if it's playing, and waits until the pause shows. Then, once a byte comes on standard
/// input, which is when `dictation` lets go of the pause, or the input ends, as it does however the app ends, plays it
/// again if it's still the Now Playing app and still paused. Returns at once where it pauses nothing.
#[unsafe(no_mangle)]
extern "C" fn roundhouse_pause_while_held() {
    let Some(media) = MediaRemote::load() else { return };
    let Some(paused) = media.now_playing().and_then(NowPlaying::app_to_pause) else { return };
    media.send(Command::Pause);
    wait_until_paused(&paused, || media.now_playing(), ASKED_EVERY, PAUSE_SHOWS_WITHIN);
    // Returns on a byte, at the end of the input, or where it can't be read, which is an end too.
    let _ = io::stdin().read_exact(&mut [0; 1]);
    if media.now_playing().is_some_and(|now| now.plays_again(&paused)) {
        media.send(Command::Play);
    }
}
