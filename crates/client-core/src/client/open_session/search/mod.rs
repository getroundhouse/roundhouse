//! Looking through all of what a view follows for a needle, when the log holds only a window of it: the hub says which
//! of the events hold it, by place, and a search reads each with what's around it, a stretch at a time, for a view to
//! count what the rows it doesn't hold draw, then let go. A chat has one search at the hub at a time, so a query typed a letter at a
//! time doesn't queue a look through the whole chat for each.

use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use protocol::event::SessionEvent;
use protocol::find::Needle;
use protocol::model::SessionId;

use super::{View, read_range};
use crate::client::follow::Teller;
use crate::session_log::Position;
use crate::{Client, ClientError, Result};

#[cfg(test)]
mod tests;

/// The most events read at once, well within what a log holds.
const LONGEST: usize = 1000;

/// What a session's searches share.
#[derive(Default)]
pub(super) struct Searches {
    /// Held while the hub is asked, so it's asked one search at a time, and one that's gone by its turn isn't asked.
    asking: Mutex<()>,
    /// The hub said it can't find, so it isn't asked again until there's a new connection, which may be to a newer one.
    cant_find: AtomicBool,
}

impl Searches {
    pub(super) fn reconnected(&self) {
        self.cant_find.store(false, Ordering::SeqCst);
    }
}

/// How far a search has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Searched {
    /// The hub is being asked, or what it named is being read.
    Underway,
    /// Every event that holds the needle is in the log, or has been read and taken.
    Done,
    /// It's shorter than [`Search::SHORTEST`], so the hub wasn't asked.
    TooShort,
    /// The hub is too old to find.
    CantFind,
    /// Asking the hub or reading what it named failed, as [`Search::take_error`] says.
    Failed,
}

/// A stretch of a session, read for a search.
#[derive(Debug)]
pub struct Stretch {
    /// Every event in the stretch, oldest first.
    pub events: Vec<SessionEvent>,
    /// Where those the hub named that it was read for are in the log, by their places, each with every event within the
    /// search's `around` places of it, but for any before the first or after the last. An event the hub named is in one
    /// stretch's `holding`, though it can be in another's `events` too.
    pub holding: Vec<Position>,
}

/// A search of a session, from [`super::OpenSession::search`], on a thread of its own, which stops once this has gone.
pub struct Search {
    progress: Arc<Mutex<Progress>>,
    stretches: flume::Receiver<Stretch>,
}

struct Progress {
    searched: Searched,
    error: Option<ClientError>,
}

impl Search {
    /// The fewest characters the hub is asked to find. One is in nearly every event, so the hub would name them all,
    /// and the whole chat would be read to count them.
    pub const SHORTEST: usize = 2;

    /// One that has got as far as it will without asking the hub.
    pub(super) fn finished(searched: Searched) -> Self {
        let (_, stretches) = flume::bounded(1);
        Self { progress: Arc::new(Mutex::new(Progress { searched, error: None })), stretches }
    }

    /// How far it has got. It's done once the last stretch has been taken.
    pub fn searched(&self) -> Searched {
        let searched = self.progress.lock().searched;
        // It says it's done after it has sent the last, so a stretch still here is seen.
        if searched == Searched::Done && !self.stretches.is_empty() { Searched::Underway } else { searched }
    }

    /// The next stretch read, if one's here. The one after is read while this one waits, and no further, so a search
    /// holds at most two.
    pub fn take(&self) -> Option<Stretch> {
        self.stretches.try_recv().ok()
    }

    /// Why it failed, the first time it's asked.
    pub fn take_error(&self) -> Option<ClientError> {
        self.progress.lock().error.take()
    }
}

/// What a search asks the hub, and reads.
pub(super) struct Asking {
    pub(super) session_id: SessionId,
    /// The events it looks through, by place.
    pub(super) view: View,
    pub(super) needle: Needle,
    /// How many places either side of each event it names are read with it.
    pub(super) around: usize,
}

/// A search on its thread.
pub(super) struct Job {
    client: Client,
    asking: Asking,
    searches: Arc<Searches>,
    progress: Arc<Mutex<Progress>>,
    stretches: flume::Sender<Stretch>,
    teller: Teller,
}

impl Job {
    pub(super) fn start(client: &Client, asking: Asking, searches: &Arc<Searches>, teller: Teller) -> Search {
        let progress = Arc::new(Mutex::new(Progress { searched: Searched::Underway, error: None }));
        let (sender, stretches) = flume::bounded(1);
        let job = Job {
            client: client.clone(),
            asking,
            searches: searches.clone(),
            progress: progress.clone(),
            stretches: sender,
            teller,
        };
        if let Err(e) = std::thread::Builder::new().name("search hub".to_string()).spawn(move || job.run()) {
            *progress.lock() = Progress { searched: Searched::Failed, error: Some(ClientError::Io(e)) };
        }
        Search { progress, stretches }
    }

    fn run(self) {
        let searched = self.search().unwrap_or_else(|e| {
            self.progress.lock().error = Some(e);
            Searched::Failed
        });
        self.progress.lock().searched = searched;
        self.teller.tell();
    }

    /// Asks the hub, then reads what it named. Once the search has gone, it stops, saying it's still underway, since
    /// there's nobody to hear.
    fn search(&self) -> Result<Searched> {
        let Asking { session_id, view, needle, around } = &self.asking;
        let found = {
            let _turn = self.searches.asking.lock();
            if self.gone() {
                return Ok(Searched::Underway);
            }
            if self.searches.cant_find.load(Ordering::SeqCst) {
                return Ok(Searched::CantFind);
            }
            match self.client.session_find(session_id, view.subagent(), needle.query()) {
                Ok(found) => found,
                Err(e) if e.is_unsupported() => {
                    self.searches.cant_find.store(true, Ordering::SeqCst);
                    return Ok(Searched::CantFind);
                }
                Err(e) => return Err(e),
            }
        };
        // A search reads by place, so the places the hub names are the positions in the log.
        let positions: Vec<Position> = found.places.iter().copied().map(Position::of_place).collect();
        for (range, holding) in stretches(&positions, *around) {
            if self.gone() {
                return Ok(Searched::Underway);
            }
            let events = read_range(&self.client, session_id, view, range)?;
            // It waits here while the last one is still to be taken.
            if self.stretches.send(Stretch { events, holding }).is_err() {
                return Ok(Searched::Underway);
            }
            self.teller.tell();
        }
        Ok(Searched::Done)
    }

    fn gone(&self) -> bool {
        self.stretches.is_disconnected()
    }
}

/// The stretches to read for `positions`, oldest first, each with every event within `around` places of those it's read
/// for, and none longer than [`LONGEST`]. Where a run of them is too long for one, the next starts back far enough to
/// have what's before its first, so each event is read with everything around it. The hub names no event at 0, which
/// has no row, so there's nothing to read for one.
fn stretches(positions: &[Position], around: usize) -> Vec<(Range<Position>, Vec<Position>)> {
    let around = around.min((LONGEST - 1) / 2);
    let mut out: Vec<(Range<usize>, Vec<Position>)> = Vec::new();
    for (&position, row) in positions.iter().filter_map(|position| Some((position, position.row()?))) {
        let (start, end) = (row.saturating_sub(around), row + around + 1);
        match out.last_mut() {
            Some((rows, holding)) if start <= rows.end && end - rows.start <= LONGEST => {
                rows.end = end;
                holding.push(position);
            }
            Some(_) | None => out.push((start..end, vec![position])),
        }
    }
    out.into_iter().map(|(rows, holding)| (Position::of_rows(rows), holding)).collect()
}
