use super::super::{Client, ClientError};
use crate::file_text::FileText;
use parking_lot::Mutex;
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
    /// The hub is too old to open files.
    Unsupported,
    /// It couldn't be opened, such as when it isn't there, or the connection went, so it's opened again on the next
    /// connection it's given.
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
    pub(super) file: (WorkspaceId, CommitId, String),
    pub(super) state: Mutex<State>,
    pub(super) changed: (flume::Sender<()>, flume::Receiver<()>),
}

pub(super) struct State {
    pub(super) opening: Opening,
    pub(super) client: Client,
    /// The lines a view has asked for since the thread last looked.
    pub(super) wanted: Vec<Range<u32>>,
    pub(super) errors: Vec<ClientError>,
}

impl OpenFile {
    /// Opens the file at `path` as `commit` held it, in the workspace `id`'s repository, on a thread of its own, so
    /// this doesn't wait for the hub.
    pub fn start(client: &Client, id: &WorkspaceId, commit: &CommitId, path: &str) -> OpenFile {
        let state = State { opening: Opening::Opening, client: client.clone(), wanted: Vec::new(), errors: Vec::new() };
        let shared = Arc::new(Shared {
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

    /// Carries on on a new connection, opening the file again if it couldn't be opened before, and says whether it is.
    pub fn reconnect(&self, client: &Client) -> bool {
        let mut state = self.shared.state.lock();
        state.client = client.clone();
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
    let client = {
        let state = shared.state.lock();
        let Opening::Opening = state.opening else { return };
        state.client.clone()
    };
    let (opened, error) = match client.workspace_file_open_at(id, commit, path) {
        Ok(FileOpened::Text(opened)) => (Opening::Text(FileText::new(opened)), None),
        Ok(FileOpened::NotText | FileOpened::Unknown) => (Opening::NotText, None),
        Err(e) if e.is_unsupported() => (Opening::Unsupported, None),
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
    let (client, page_lines, asked) = {
        let mut state = shared.state.lock();
        let State { opening: Opening::Text(text), client, wanted, .. } = &mut *state else { return false };
        let asked = text.to_fetch(&std::mem::take(wanted));
        if asked.is_empty() {
            return false;
        }
        (client.clone(), text.page_lines(), asked)
    };
    let read = client.workspace_file_pages(id, commit, path, page_lines, &asked);
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
