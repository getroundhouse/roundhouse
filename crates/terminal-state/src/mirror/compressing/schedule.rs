//! When a copy compresses its history, and how much at a time.

use std::cell::Cell;
use std::time::{Duration, Instant};

use libghostty_vt::terminal::{CompressionActivity, CompressionMode, CompressionResult};

use crate::{Mirror, MirrorError};

/// How long a terminal is quiet before its history is compressed, so one a program is writing to isn't compressed
/// between its writes. Compressing a full history takes a few milliseconds, so it needn't be long.
pub const QUIET_BEFORE_COMPRESSING: Duration = Duration::from_secs(5);
/// The most of Ghostty's steps one call takes, each under 50 µs on a Mac, so the copy's thread draws and takes in
/// output between them. A full history at 200 columns takes about 50.
const STEPS: usize = 20;

/// Where a copy's compressing has got to.
#[derive(Debug, Default)]
pub(in crate::mirror) struct Compressing {
    /// None until the copy's first asked when to compress.
    since: Option<Since>,
    /// Whether the whole history has been read since, as a large selection's text and rows of history read it.
    /// Reading it decompresses it without changing Ghostty's token, so a read counts as a change. It's a `Cell` since
    /// reading takes the copy as it is.
    read: Cell<bool>,
}

/// When the terminal last changed, as far as the copy has seen, and what it has done about it since.
#[derive(Debug, Clone, Copy)]
struct Since {
    /// Ghostty's token for what the terminal last did, which changes as its history does, and not as it's compressed.
    activity: CompressionActivity,
    at: Instant,
    /// Whether a pass has compressed everything it could since. Asked again, Ghostty would start another.
    done: bool,
    /// Whether Ghostty can't compress here at all, which it says when asked to.
    unsupported: bool,
}

impl Mirror {
    /// When to call [`Mirror::compress`] next, as of `now`: once the terminal has been quiet for
    /// [`QUIET_BEFORE_COMPRESSING`], and then at once, until its history is compressed as far as it goes. None from
    /// then until it changes, or its history is read whole, and always where Ghostty can't compress.
    pub fn compress_at(&mut self, now: Instant) -> Result<Option<Instant>, MirrorError> {
        let activity = self.terminal.compression_activity()?;
        let read = self.compressing.read.take();
        let since = self.compressing.since.get_or_insert(Since { activity, at: now, done: false, unsupported: false });
        if since.activity != activity || read {
            *since = Since { activity, at: now, done: false, ..*since };
        }
        Ok((!since.done && !since.unsupported).then_some(since.at + QUIET_BEFORE_COMPRESSING))
    }

    /// Compresses some of the history, if it's time to, as [`Mirror::compress_at`] says.
    pub fn compress(&mut self, now: Instant) -> Result<(), MirrorError> {
        if self.compress_at(now)?.is_none_or(|at| at > now) {
            return Ok(());
        }
        for _ in 0..STEPS {
            let compressed = self.terminal.compress(CompressionMode::Incremental);
            let Some(since) = &mut self.compressing.since else { return Ok(()) };
            match compressed {
                Ok(CompressionResult::Pending) => {}
                Ok(CompressionResult::Complete) => {
                    since.done = true;
                    return Ok(());
                }
                Ok(CompressionResult::Unsupported) => {
                    since.unsupported = true;
                    return Ok(());
                }
                // Until the terminal changes, rather than trying again at once.
                Err(e) => {
                    since.done = true;
                    return Err(e.into());
                }
            }
        }
        Ok(())
    }

    /// Notes that the whole history has been read, which decompresses it.
    pub(in crate::mirror) fn history_read(&self) {
        self.compressing.read.set(true);
    }
}
