//! Pictures of what a terminal shows, sent to the hub while it watches, one frame on its way at a time, at most about
//! 60 a second, and fewer of a screen that takes long to read. Under a flood the worker goes on taking in all its
//! program writes, and the hub is sent the newest of what the terminal shows whenever there's room, rather than all
//! that came between.

use std::sync::Arc;
use std::time::Duration;

use protocol::model::TerminalId;
use protocol::terminal::{PictureSending, TerminalPicture};
use protocol::worker::WorkerFrame;
use terminal_state::Mirror;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;
use tracing::warn;

use crate::serve::{HubConnection, Outbox};

/// How long after a picture begins the next can, which is about 60 a second. A change to a terminal that has been
/// still for longer than the next may wait, such as a key's echo, goes at once.
const PICTURE_EVERY: Duration = Duration::from_millis(16);
/// How many times as long as the last picture took to make the next waits at least, so pictures take at most about a
/// tenth of the terminal's thread, and the rest goes to taking in what its program writes. Reading a screen of 600 by
/// 200 with a colour in every cell takes about 14 ms in a release build, which would be most of a core at 60 a second.
const PICTURE_SHARE: u32 = 10;
/// The longest the next picture waits for the last's share, so one that took long, as on a thread made to wait for a
/// core, holds back none for long.
const SHARE_AT_MOST: Duration = Duration::from_millis(500);
/// The longest a program that asked for its output to be shown all at once holds pictures back, as Ghostty holds its
/// own drawing back.
const SYNCHRONIZED_AT_MOST: Duration = Duration::from_secs(1);

pub(super) struct Watching {
    terminal: TerminalId,
    outbox: Arc<Outbox>,
    /// The hub that watches, while one does.
    hub: Option<HubConnection>,
    sending: PictureSending,
    /// Room for the one frame of the terminal's pictures that can be on its way.
    room: Arc<Semaphore>,
    /// Whether what the terminal shows may have changed since the last picture began.
    changed: bool,
    /// When the last picture began, and how long it took to make.
    began: Option<Instant>,
    took: Duration,
    /// Since when the program has asked for its output to be shown all at once.
    synchronized: Option<Instant>,
}

impl Watching {
    pub(super) fn new(terminal: TerminalId, outbox: Arc<Outbox>) -> Self {
        let (sending, room) = (PictureSending::default(), Arc::new(Semaphore::new(1)));
        let (began, took, synchronized) = (None, Duration::ZERO, None);
        Self { terminal, outbox, hub: None, sending, room, changed: false, began, took, synchronized }
    }

    /// Watched by `hub` from now, with a whole picture first, or by none.
    pub(super) fn watch(&mut self, hub: Option<HubConnection>) {
        self.hub = hub;
        self.sending.reset();
        self.changed = true;
    }

    /// Notes that what the terminal shows may have changed.
    pub(super) fn changed(&mut self, mirror: &Mirror) {
        self.changed = true;
        self.synchronized = mirror.synchronized().then(|| self.synchronized.unwrap_or_else(Instant::now));
    }

    /// When the next frame is due, if one may be: the rest of a picture at once, and a new one once it's paced, and
    /// the program isn't holding it back.
    pub(super) fn due(&self) -> Option<Instant> {
        self.hub?;
        let now = Instant::now();
        if self.sending.sending() {
            return Some(now);
        }
        if !self.changed {
            return None;
        }
        let paced = self.began.map(|began| began + PICTURE_EVERY.max((self.took * PICTURE_SHARE).min(SHARE_AT_MOST)));
        let held = self.synchronized.map(|since| since + SYNCHRONIZED_AT_MOST);
        Some(paced.into_iter().chain(held).fold(now, Instant::max))
    }

    /// Room for the next frame, to wait for with whatever else the terminal waits for.
    pub(super) fn room(&self) -> Arc<Semaphore> {
        self.room.clone()
    }

    /// Sends the next frame, if the hub lacks anything, holding `written` until it has been written.
    pub(super) async fn send(&mut self, mirror: &mut Mirror, written: OwnedSemaphorePermit) {
        let Some(hub) = self.hub else { return };
        let picture = self.sending.rest().or_else(|| self.begin(mirror));
        let Some(picture) = picture else { return };
        let frame = WorkerFrame::TerminalPicture { terminal: self.terminal.clone(), picture };
        if !self.outbox.send_to(hub, frame, written).await {
            // That hub has gone, and one that connects watches again.
            self.watch(None);
        }
    }

    /// A new picture's first frame, of what the terminal shows now, if the hub lacks any of it.
    fn begin(&mut self, mirror: &mut Mirror) -> Option<TerminalPicture> {
        self.changed = false;
        let began = Instant::now();
        let picture = match mirror.shown() {
            Ok(now) => self.sending.next(now, |number| mirror.history_row(number)),
            Err(e) => {
                warn!(
                    name: "terminal.picture_failed",
                    terminal = %self.terminal,
                    error = %e,
                    "Couldn't say what a terminal shows"
                );
                None
            }
        };
        (self.began, self.took) = (Some(began), began.elapsed());
        picture
    }
}
