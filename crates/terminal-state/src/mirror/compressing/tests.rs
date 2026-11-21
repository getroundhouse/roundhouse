use std::time::{Duration, Instant};

use libghostty_vt::fmt::{Extras, Format, Formatter};
use protocol::terminal::TerminalSize;

use crate::{Mirror, QUIET_BEFORE_COMPRESSING};

const SIZE: TerminalSize = TerminalSize { cols: 200, rows: 50 };

/// A copy whose history is full, as one written to for a while is.
fn full() -> Mirror {
    let mut mirror = Mirror::new(SIZE).unwrap();
    for line in 0..12_000 {
        mirror.write(format!("\x1b[32mline {line}\x1b[0m of a build's output, as long as most are\r\n").as_bytes());
    }
    mirror
}

/// The screen showing, with its history, and its colours, as escape sequences.
fn styled(mirror: &Mirror) -> Vec<u8> {
    let mut formatter = Formatter::new(&mirror.terminal).unwrap();
    formatter.set_format(Format::Vt).unwrap().set_extras(Extras::ALL).unwrap().set_trim(false).unwrap();
    formatter.format_alloc(None).unwrap().to_vec()
}

/// Whether Ghostty can compress here, which isn't so everywhere.
fn compresses(mirror: &Mirror) -> bool {
    mirror.terminal.memory_usage().unwrap().compression_supported
}

fn held(mirror: &Mirror) -> u64 {
    mirror.terminal.memory_usage().unwrap().primary_resident_bytes
}

/// Calls `compress` each time `compress_at` says to, from `now`, until it says there's no more to do. Returns when that
/// was, and how many calls it took.
fn compress_until_done(mirror: &mut Mirror, now: Instant) -> (Instant, usize) {
    let (mut now, mut calls) = (now, 0);
    while let Some(at) = mirror.compress_at(now).unwrap() {
        now = now.max(at);
        mirror.compress(now).unwrap();
        calls += 1;
        assert!(calls < 1000, "compressing never finishes");
    }
    (now, calls)
}

/// Once its terminal has been quiet a while, then a few steps at a time until there's no more, then not again until
/// the terminal changes or its history is read whole.
#[test]
fn a_copy_compresses_its_history_once_its_terminal_is_quiet() {
    let start = Instant::now();
    let mut mirror = full();
    let quiet = start + QUIET_BEFORE_COMPRESSING;
    assert_eq!(mirror.compress_at(start).unwrap(), Some(quiet));
    let before = held(&mirror);

    mirror.compress(start + QUIET_BEFORE_COMPRESSING / 2).unwrap();
    let too_soon = held(&mirror);
    let (done, calls) = compress_until_done(&mut mirror, start);

    assert_eq!(too_soon, before, "nothing compressed before it's quiet");
    assert_eq!(done, quiet, "and then each step at once");
    if compresses(&mirror) {
        assert!(held(&mirror) < before / 4, "{} bytes, from {before}", held(&mirror));
        assert!(calls > 1, "a few steps at a time");
        let later = done + Duration::from_secs(60);
        mirror.write(b"more output\r\n");
        assert_eq!(mirror.compress_at(later).unwrap(), Some(later + QUIET_BEFORE_COMPRESSING), "once it changes");
        compress_until_done(&mut mirror, later);
        mirror.select_all();
        mirror.selected_text();
        assert_eq!(mirror.compress_at(later).unwrap(), Some(later + QUIET_BEFORE_COMPRESSING), "once it's read whole");
        compress_until_done(&mut mirror, later);
        mirror.resize(TerminalSize { cols: 120, ..SIZE }).unwrap();
        assert_eq!(mirror.compress_at(later).unwrap(), Some(later + QUIET_BEFORE_COMPRESSING), "once it's resized");
        compress_until_done(&mut mirror, later);
    }
    // Asked again without a change, Ghostty would start another pass, which `compress_at` doesn't ask for.
    assert_eq!(compress_until_done(&mut mirror, done + Duration::from_secs(3600)).1, 0);
}

/// Compressing changes only how the history is held, whatever's written to it after, and at a size that lays it out
/// again.
#[test]
fn a_compressed_copy_shows_writes_and_copies_what_it_did() {
    let shown = |mirror: &mut Mirror| {
        mirror.write(b"more output\r\n");
        mirror.resize(TerminalSize { cols: 120, ..SIZE }).unwrap();
        mirror.select_all();
        (styled(mirror), mirror.selected_text())
    };
    let mut compressed = full();
    compress_until_done(&mut compressed, Instant::now());

    assert_eq!(shown(&mut compressed), shown(&mut full()));
}
