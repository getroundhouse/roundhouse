use super::super::ClientError;
use crate::Hub;
use crate::file_text::FileText;
use parking_lot::Mutex;
use protocol::client::files::{WorkspaceFileOpenAt, WorkspaceFilePages};
use protocol::model::{CommitId, FileOpened, WorkspaceId};
use std::ops::Range;
use std::sync::Arc;

/// What's known of a file someone has open.
#[derive(Clone, Debug)]
pub enum Opening {
    /// It's being opened.
    Opening,
    Text(FileText),
    /// It's an image, or something else that isn't text.
    NotText,
    /// It couldn't be opened, such as when it isn't there, or there was no connection that had caught up, so it's
    /// opened again when it's next asked to be, with [`OpenFile::open_again`].
    Failed,
}

/// A file someone has open, read until every clone of this has gone.
#[derive(Clone)]
pub struct OpenFile {
    pub(super) shared: Arc<Shared>,
    /// Wakes the thread, which ends once every clone of this has gone.
    pub(super) wake: flume::Sender<()>,
}

pub(super) struct Shared {
    /// What it's read from.
    pub(super) hub: Hub,
    pub(super) file: (WorkspaceId, CommitId, String),
    pub(super) state: Mutex<State>,
    pub(super) changed: (flume::Sender<()>, flume::Receiver<()>),
}

pub(super) struct State {
    pub(super) opening: Opening,
    /// The lines a view has asked for since the thread last looked.
    pub(super) wanted: Vec<Range<u32>>,
    pub(super) errors: Vec<ClientError>,
}

impl OpenFile {
    /// Opens the file at `path` as `commit` held it, in the workspace `id`'s repository, on a thread of its own, so
    /// this doesn't wait for the hub. It reads from `hub` as [`Hub::call`] does. A view opens one from its [`Hub`].
    pub(crate) fn start(hub: &Hub, id: &WorkspaceId, commit: &CommitId, path: &str) -> OpenFile {
        let state = State { opening: Opening::Opening, wanted: Vec::new(), errors: Vec::new() };
        let shared = Arc::new(Shared {
            hub: hub.clone(),
            file: (id.clone(), commit.clone(), path.to_string()),
            state: Mutex::new(state),
            changed: flume::bounded(1),
        });
        // One wake at a time, so asks that come while it's busy gather into one.
        let (wake, woken) = flume::bounded(1);
        let reading = shared.clone();
        std::thread::spawn(move || read(&reading, &woken));
        OpenFile { shared, wake }
    }

    /// Says that something changed. It holds one signal at a time, so changes a view hasn't looked at yet gather into
    /// one, and each signal goes to one receiver, so one view listens.
    pub fn changed(&self) -> &flume::Receiver<()> {
        &self.shared.changed.1
    }

    pub fn read<T>(&self, read: impl FnOnce(&Opening) -> T) -> T {
        read(&self.shared.state.lock().opening)
    }

    /// Fetches the pages `lines`, by number, are on that aren't held or being fetched, once the file is open.
    pub fn want(&self, lines: &[Range<u32>]) {
        self.shared.state.lock().wanted.extend_from_slice(lines);
        let _ = self.wake.try_send(());
    }

    /// Opens the file again if it couldn't be opened before, as when there was no connection that had caught up then,
    /// and says whether it is.
    pub fn open_again(&self) -> bool {
        let mut state = self.shared.state.lock();
        let again = matches!(state.opening, Opening::Failed);
        if again {
            state.opening = Opening::Opening;
        }
        drop(state);
        let _ = self.wake.try_send(());
        again
    }

    /// What went wrong opening it or fetching its pages, for a person to hear about. A page that couldn't be fetched
    /// is fetched again the next time it's wanted.
    pub fn take_errors(&self) -> Vec<ClientError> {
        std::mem::take(&mut self.shared.state.lock().errors)
    }
}

impl Shared {
    pub(super) fn tell(&self) {
        let _ = self.changed.0.try_send(());
    }
}

/// Opens the file, and then fetches what's wanted each time it's woken, until every [`OpenFile`] has gone.
fn read(shared: &Shared, woken: &flume::Receiver<()>) {
    loop {
        open(shared);
        // Nothing more to do until a view asks for more, or a new connection comes.
        if !fetch(shared) && woken.recv().is_err() {
            return;
        }
    }
}

/// Opens the file, if it's to be opened.
fn open(shared: &Shared) {
    let (id, commit, path) = &shared.file;
    if !matches!(shared.state.lock().opening, Opening::Opening) {
        return;
    }
    let asked = WorkspaceFileOpenAt { id: id.clone(), commit: commit.clone(), path: path.clone() };
    let (opened, error) = match shared.hub.call(asked) {
        Ok(FileOpened::Text(opened)) => (Opening::Text(FileText::new(opened)), None),
        Ok(FileOpened::NotText | FileOpened::Unknown) => (Opening::NotText, None),
        Err(e) => (Opening::Failed, Some(e)),
    };
    let mut state = shared.state.lock();
    state.opening = opened;
    state.errors.extend(error);
    drop(state);
    shared.tell();
}

/// Fetches the pages the lines wanted are on, once the file is open, and says whether there were any to fetch. Those
/// that don't come are fetched again when they're next wanted.
fn fetch(shared: &Shared) -> bool {
    let (id, commit, path) = &shared.file;
    let (page_lines, asked) = {
        let mut state = shared.state.lock();
        let State { opening: Opening::Text(text), wanted, .. } = &mut *state else { return false };
        let asked = text.to_fetch(&std::mem::take(wanted));
        if asked.is_empty() {
            return false;
        }
        (text.page_lines(), asked)
    };
    let (id, commit, path, pages) = (id.clone(), commit.clone(), path.clone(), asked.clone());
    let read = shared.hub.call(WorkspaceFilePages { id, commit, path, page_lines, pages });
    let mut state = shared.state.lock();
    let State { opening: Opening::Text(text), errors, .. } = &mut *state else { return true };
    let (pages, error) = match read {
        Ok(read) => (read.pages, None),
        Err(e) => (Vec::new(), Some(e)),
    };
    let taken = text.fetched(&asked, pages);
    errors.extend(error);
    drop(state);
    if !taken.is_empty() {
        shared.tell();
    }
    true
}
