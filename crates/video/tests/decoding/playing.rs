use std::io::Cursor;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;
use video::{Decoded, Player, WebmVideo};

use super::{THREADS, VP8_KEYFRAMES};

/// Far longer than decoding a fixture's frame takes, on a loaded machine.
const LONG: Duration = Duration::from_secs(10);

/// A player of `bytes`, and what it's woken by.
fn playing(bytes: &'static [u8]) -> (Player, Receiver<()>) {
    let (woke, woken) = mpsc::channel();
    let open = move || WebmVideo::open(Cursor::new(bytes), THREADS);
    let player = Player::start(open, 3, move || {
        let _ = woke.send(());
    })
    .unwrap();
    (player, woken)
}

/// The next thing the player's thread did, waiting for it.
fn next(player: &Player, woken: &Receiver<()>) -> Decoded {
    loop {
        if let Some(decoded) = player.take() {
            return decoded;
        }
        woken.recv_timeout(LONG).expect("the player said nothing");
    }
}

/// The time of the next frame, and the seek it follows, or what came instead.
fn frame(player: &Player, woken: &Receiver<()>) -> (u64, Duration) {
    match next(player, woken) {
        Decoded::Frame(shown) => {
            assert_eq!((shown.width, shown.height, shown.bgra.len()), (64, 36, 64 * 36 * 4));
            (shown.seek, shown.time)
        }
        other => panic!("{other:?}"),
    }
}

/// It gives every frame in order, ready to draw, then its end, and after a seek, the frames from there, which say they
/// follow it.
#[test]
fn a_player_gives_each_frame_in_order_and_goes_where_it_s_asked() {
    let (mut player, woken) = playing(VP8_KEYFRAMES);

    let first = [frame(&player, &woken), frame(&player, &woken)];
    let seek = player.seek(Duration::from_millis(1750));
    let after: Vec<_> =
        std::iter::repeat_with(|| frame(&player, &woken)).skip_while(|(at, _)| *at != seek).take(2).collect();
    let end = next(&player, &woken);

    assert_eq!(first, [(0, Duration::ZERO), (0, Duration::from_millis(100))]);
    assert_eq!(after, [(seek, Duration::from_millis(1800)), (seek, Duration::from_millis(1900))]);
    assert!(matches!(end, Decoded::End { seek: ended } if ended == seek), "{end:?}");
}

/// A file that isn't a video it plays says why, and a player dropped stops its thread.
#[test]
fn a_player_says_why_it_can_t_play_and_stops_once_dropped() {
    let (broken, woken) = playing(b"\x89PNG\r\n\x1a\n");
    let failed = next(&broken, &woken);
    let (player, woken) = playing(VP8_KEYFRAMES);
    next(&player, &woken);

    drop(player);

    assert!(matches!(failed, Decoded::Unopened), "{failed:?}");
    loop {
        match woken.recv_timeout(LONG) {
            Ok(()) => {}
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => panic!("the thread went on"),
        }
    }
}
