//! What every watch of a worktree shares of what the OS says: whether it can have changed anything, and waiting for
//! what it heard to settle, since the OS says a file changed, and the watch reads only once the writes have stopped,
//! or once it's waited long enough to read anyway.

use notify::event::{AccessKind, AccessMode};
use notify::{Event, EventKind};
use std::time::Duration;
use tokio::sync::{Notify, watch};

/// What the OS told a watch, before the watch looks at which paths it's of.
#[derive(Debug)]
pub(crate) enum Told<'a> {
    /// The OS dropped what it had to say, as when its queue fills, or the watcher failed, so anything may have changed.
    LostTrack,
    /// A file was opened, read or closed without being written to, which inotify tells of too, and which changes
    /// nothing. Every git command opens `HEAD` and the config, and a watch reads what it watches, so counting these,
    /// a watch's own reads would wake it again, and every other worker's on the same files, for ever.
    NothingWritten,
    /// Something may have changed at the event's paths.
    Changed(&'a Event),
}

impl<'a> Told<'a> {
    pub(crate) fn of(event: &'a notify::Result<Event>) -> Self {
        let Ok(event) = event else { return Self::LostTrack };
        if event.need_rescan() {
            return Self::LostTrack;
        }
        if let EventKind::Access(access) = event.kind
            && access != AccessKind::Close(AccessMode::Write)
        {
            return Self::NothingWritten;
        }
        Self::Changed(event)
    }
}

/// How a watch waits before it reads again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Settling {
    /// How long nothing has to have been heard, after something was, before it reads, since a checkout or a save
    /// writes several files one after another.
    pub(crate) quiet: Duration,
    /// How long after the first thing heard it reads anyway, however much it goes on hearing, so a run of writes that
    /// never stops for `quiet` is still read. `None` waits for the quiet however long that takes.
    pub(crate) at_most: Option<Duration>,
    /// How long it goes before reading again, whatever it heard. The OS can drop what it says, as inotify does when its
    /// queue fills, and some filesystems, such as a network's or one shared into a VM, don't tell it of every change.
    pub(crate) at_least_every: Duration,
}

/// Why a watch reads again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Woke {
    /// Something it heard has settled.
    Heard,
    /// It's gone `at_least_every` hearing nothing.
    Due,
    /// The worker is stopping, so it reads nothing more.
    Stopping,
}

impl Settling {
    /// Waits for `wake` to be told, and then for it to settle, or for long enough to read anyway. `wake` holds one
    /// telling while nothing waits, so what was heard while the watch was reading wakes this at once.
    #[expect(
        clippy::disallowed_methods,
        reason = "a watch reads at least every `at_least_every`, and once what it heard has been quiet for `quiet`"
    )]
    pub(crate) async fn wait(&self, wake: &Notify, stopping: &mut watch::Receiver<bool>) -> Woke {
        tokio::select! {
            () = wake.notified() => {}
            () = tokio::time::sleep(self.at_least_every) => return Woke::Due,
            _ = stopping.wait_for(|stopping| *stopping) => return Woke::Stopping,
        }
        let at_most = async {
            match self.at_most {
                Some(at_most) => tokio::time::sleep(at_most).await,
                None => std::future::pending().await,
            }
        };
        tokio::pin!(at_most);
        loop {
            tokio::select! {
                () = wake.notified() => {}
                () = tokio::time::sleep(self.quiet) => return Woke::Heard,
                () = &mut at_most => return Woke::Heard,
                _ = stopping.wait_for(|stopping| *stopping) => return Woke::Stopping,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, DataChange, Flag, ModifyKind};
    use std::sync::Arc;

    /// A write, a file's close after one, and a file made can each have changed something, a read or a close after one
    /// can't, and the OS losing track or failing may have changed anything.
    #[test]
    fn only_what_writes_or_loses_track_is_told_as_a_change() {
        let event = |kind| Ok(Event::new(kind).add_path("/w/a.rs".into()));
        let cases = [
            (event(EventKind::Modify(ModifyKind::Data(DataChange::Any))), "changed"),
            (event(EventKind::Access(AccessKind::Close(AccessMode::Write))), "changed"),
            (event(EventKind::Create(CreateKind::File)), "changed"),
            (event(EventKind::Access(AccessKind::Open(AccessMode::Read))), "nothing written"),
            (event(EventKind::Access(AccessKind::Close(AccessMode::Read))), "nothing written"),
            (Ok(Event::new(EventKind::Other).set_flag(Flag::Rescan)), "lost track"),
            (Err(notify::Error::generic("the watcher failed")), "lost track"),
        ];

        for (event, said) in &cases {
            let told = match Told::of(event) {
                Told::LostTrack => "lost track",
                Told::NothingWritten => "nothing written",
                Told::Changed(_) => "changed",
            };
            assert_eq!(told, *said, "{event:?}");
        }
    }

    const SETTLING: Settling = Settling {
        quiet: Duration::from_millis(100),
        at_most: Some(Duration::from_secs(1)),
        at_least_every: Duration::from_mins(5),
    };

    /// What `wait` returns, and how long after it began, on tokio's paused clock, while `heard` is told every
    /// `every` for `for_` from the start.
    async fn waited(settling: Settling, every: Duration, for_: Duration) -> (Woke, Duration) {
        let wake = Arc::new(Notify::new());
        let (_stop, mut stopping) = watch::channel(false);
        let telling = wake.clone();
        let tells = tokio::spawn(async move {
            let started = tokio::time::Instant::now();
            while started.elapsed() <= for_ {
                telling.notify_one();
                #[expect(clippy::disallowed_methods, reason = "writes heard at a pace, on a paused clock")]
                tokio::time::sleep(every).await;
            }
        });
        tokio::task::yield_now().await;
        let started = tokio::time::Instant::now();
        let returned = settling.wait(&wake, &mut stopping).await;
        tells.abort();
        (returned, started.elapsed())
    }

    /// A short run of writes is read once it's been quiet for `quiet`, and one that never stops is read at `at_most`,
    /// or, for a watch with none, not until it stops.
    #[tokio::test(start_paused = true)]
    async fn a_watch_reads_once_writes_stop_or_once_they_have_gone_on_too_long() {
        let quick = waited(SETTLING, Duration::from_millis(10), Duration::from_millis(50)).await;
        let endless = waited(SETTLING, Duration::from_millis(10), Duration::from_mins(1)).await;
        let patient = Settling { at_most: None, ..SETTLING };
        let long = waited(patient, Duration::from_millis(10), Duration::from_secs(3)).await;

        assert_eq!(quick.0, Woke::Heard);
        assert!((Duration::from_millis(100)..Duration::from_millis(200)).contains(&quick.1), "{:?}", quick.1);
        assert_eq!(endless, (Woke::Heard, Duration::from_secs(1)));
        assert_eq!(long.0, Woke::Heard);
        assert!(long.1 > Duration::from_secs(3), "{:?}", long.1);
    }

    /// With nothing heard, it reads anyway at `at_least_every`.
    #[tokio::test(start_paused = true)]
    async fn a_watch_that_hears_nothing_reads_every_few_minutes() {
        let wake = Notify::new();
        let (_stop, mut stopping) = watch::channel(false);
        let started = tokio::time::Instant::now();

        let returned = SETTLING.wait(&wake, &mut stopping).await;

        assert_eq!((returned, started.elapsed()), (Woke::Due, Duration::from_mins(5)));
    }
}
