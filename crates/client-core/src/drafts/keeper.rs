//! The thread that keeps what [`Drafts`](super::Drafts) is handed, one call at a time.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use protocol::model::AttachmentId;

use super::{Agreed, Counts, DraftChange, DraftKey, FilesFor, PAUSE, Request, Said, agree};
use crate::{Client, ClientError, FileToSend};

/// Starts the thread, which ends once every handle has gone.
pub(super) fn start(
    client: Client,
    requests: flume::Receiver<Request>,
    counts: Counts,
    catching_up: flume::Sender<()>,
) {
    let keeper = Keeper {
        client,
        pending: HashMap::new(),
        adding: VecDeque::new(),
        removing: Vec::new(),
        stalled: false,
        counts,
        catching_up,
        flushing: Vec::new(),
    };
    // Without the thread, nothing is kept, and the count says so, since it only goes down as the hub takes each.
    let _ = std::thread::Builder::new().name("hub-drafts".to_string()).spawn(move || keeper.run(&requests));
}

struct Keeper {
    client: Client,
    /// The newest change to each draft that isn't kept yet, and when to keep it.
    pending: HashMap<DraftKey, Pending>,
    /// Files to give the hub for what's typed where they're attached, a chat's composer or the new-workspace dialog,
    /// before any draft is kept, in the order they came.
    adding: VecDeque<(FilesFor, FileToSend)>,
    /// Files to let go of, each once the draft it was for has no change waiting to be kept.
    removing: Vec<(FilesFor, AttachmentId)>,
    /// The connection went, so nothing more is tried until the next.
    stalled: bool,
    counts: Counts,
    /// Signals [`Drafts::caught_up`](super::Drafts::caught_up).
    catching_up: flume::Sender<()>,
    /// Waiting for everything given so far to be kept.
    flushing: Vec<flume::Sender<()>>,
}

struct Pending {
    change: DraftChange,
    due: Instant,
    /// When it was handed over, which is when it was typed, as `SessionDraft` asks of a composer's change.
    typed_at_ms: i64,
}

impl Keeper {
    fn run(mut self, requests: &flume::Receiver<Request>) {
        loop {
            let next = self.pending.values().map(|p| p.due).min().filter(|_| !self.stalled);
            let request = match next {
                Some(due) => requests.recv_deadline(due),
                None => requests.recv().map_err(|_| flume::RecvTimeoutError::Disconnected),
            };
            match request {
                Ok(request) => self.take(request),
                Err(flume::RecvTimeoutError::Timeout) => {}
                // Every handle has gone. One about to go, such as an app quitting, keeps what's left first, with
                // `flushed`, since a process that's exiting wouldn't wait for this thread.
                Err(flume::RecvTimeoutError::Disconnected) => return,
            }
            let now = Instant::now();
            self.keep(|pending| pending.due <= now);
            // Everything left waits for another connection, or there's nothing left.
            let left = !self.pending.is_empty() || !self.adding.is_empty() || !self.removing.is_empty();
            if self.stalled || !left {
                for done in self.flushing.drain(..) {
                    let _ = done.send(());
                }
            }
        }
    }

    fn take(&mut self, request: Request) {
        let now = Instant::now();
        match request {
            Request::Changed { change, now: at_once, typed_at_ms } => {
                let key = change.key();
                // A change while one waits keeps its time, so typing that goes on is still kept every so often.
                let waiting = self.pending.get(&key).map_or(now + PAUSE, |p| p.due);
                let due = if at_once { now } else { waiting };
                if self.pending.insert(key.clone(), Pending { change, due, typed_at_ms }).is_some() {
                    // The one it replaces never needs to reach the hub.
                    self.reached(&key, None);
                }
            }
            Request::Attach { to, file } => self.adding.push_back((to, file)),
            Request::Detach { from, file } => self.detach(from, file),
            Request::Reconnect(client) => {
                // The dialog's copy agreed on went as the client was handed the connection, and goes again now, in
                // case the last hub's answer came since.
                (self.client, self.stalled) = (client, false);
                self.counts.lock().agreed.remove(&DraftKey::NewWorkspace);
                for pending in self.pending.values_mut() {
                    pending.due = now;
                }
            }
            Request::Flush(done) => {
                for pending in self.pending.values_mut() {
                    pending.due = now;
                }
                self.flushing.push(done);
            }
        }
    }

    /// Lets go of `file`, unless it's still waiting to be added, such as while the connection is gone, when it never
    /// needs to reach the hub. Each file attached has an ID of its own, so it's the same one.
    fn detach(&mut self, from: FilesFor, file: AttachmentId) {
        let waiting = self.adding.iter().position(|(to, waiting)| *to == from && waiting.attachment.id == file);
        if let Some(i) = waiting {
            self.adding.remove(i);
            self.reached(&from.key(), None);
            // Unless the hub has it anyway, such as a file of a prompt that came back because it couldn't be sent.
            if !matches!(&from, FilesFor::Chat(id) if self.client.has_given(id, &file)) {
                return;
            }
        }
        self.removing.push((from, file));
    }

    /// Adds every file waiting, then keeps each change `due` picks, then lets go of the files whose drafts have no
    /// draft left waiting, unless the connection goes.
    fn keep(&mut self, due: impl Fn(&Pending) -> bool) {
        self.add_files();
        self.keep_changes(due);
        self.remove_files();
    }

    fn keep_changes(&mut self, due: impl Fn(&Pending) -> bool) {
        let picked: Vec<DraftKey> = self.pending.iter().filter(|(_, p)| due(p)).map(|(key, _)| key.clone()).collect();
        for key in picked {
            if self.stalled {
                return;
            }
            let Some(pending) = self.pending.remove(&key) else { continue };
            let answer = self.send(&pending);
            let gone = matches!(
                answer,
                Err(ClientError::Io(_)
                    | ClientError::Disconnected
                    | ClientError::FingerprintMismatch { .. }
                    | ClientError::Unreachable { .. })
            );
            // For the next connection.
            if gone {
                self.pending.insert(key, pending);
                self.stalled = true;
            } else {
                // Kept, or the hub answered and won't keep this one however often it's asked, such as for a chat
                // that's closed or gone, or a composer whose draft was changed elsewhere after this was typed, when
                // the hub's draft is shown in its place.
                if matches!(key, DraftKey::Composer(_)) && answer.as_ref().is_err_and(ClientError::is_conflict) {
                    self.counts.lock().overruled.insert(key.clone());
                }
                self.reached(&key, answer.ok());
            }
        }
    }

    /// Has the hub keep `pending`'s change, and says what the record it kept it on says is there now.
    fn send(&self, pending: &Pending) -> Result<Agreed, ClientError> {
        match &pending.change {
            DraftChange::Composer(id, draft) => self
                .client
                .session_draft(id, draft, Some(pending.typed_at_ms))
                .map(|session| Agreed { rev: session.rev, said: Said::Composer(session.draft) }),
            DraftChange::Decision(id, draft) => self
                .client
                .session_decision_draft(id, draft.as_ref())
                .map(|session| Agreed { rev: session.rev, said: Said::Decision(session.decision_draft) }),
            DraftChange::Comment(id, draft) => self
                .client
                .workspace_comment_draft(id, draft.as_ref())
                .map(|workspace| Agreed { rev: workspace.rev, said: Said::Comment(workspace.comment_draft) }),
            DraftChange::NewWorkspace(draft) => self
                .client
                .new_workspace_draft_keep(draft)
                .map(|kept| Agreed { rev: kept.rev, said: Said::new_workspace(&kept) }),
        }
    }

    fn add_files(&mut self) {
        while !self.stalled
            && let Some((to, file)) = self.adding.pop_front()
        {
            let key = to.key();
            match self.add(&to, &file) {
                // Added, or the hub won't take it however often it's asked, and then a draft goes without it.
                Ok(()) | Err(ClientError::Api(_) | ClientError::Unexpected(_)) => self.reached(&key, None),
                Err(
                    ClientError::Io(_)
                    | ClientError::Disconnected
                    | ClientError::FingerprintMismatch { .. }
                    | ClientError::Unreachable { .. },
                ) => {
                    self.adding.push_front((to, file));
                    self.stalled = true;
                }
            }
        }
    }

    /// Gives the hub `file` for what's typed in `to` to name.
    fn add(&self, to: &FilesFor, file: &FileToSend) -> Result<(), ClientError> {
        let (id, name, data) = (&file.attachment.id, &file.attachment.name, &file.data);
        let added = match to {
            // Such as a file of a prompt that came back because it couldn't be sent.
            FilesFor::Chat(chat) if self.client.has_given(chat, id) => return Ok(()),
            FilesFor::Chat(chat) => file.give(|kind| self.client.attachment_add(chat, id, name, kind, data.clone())),
            FilesFor::NewWorkspace => {
                file.give(|kind| self.client.new_workspace_draft_file_add(id, name, kind, data.clone()))
            }
        };
        added.map(drop)
    }

    fn remove_files(&mut self) {
        let mut left = Vec::new();
        for (from, file) in std::mem::take(&mut self.removing) {
            let key = from.key();
            if self.stalled || self.pending.contains_key(&key) {
                left.push((from, file));
                continue;
            }
            let removed = match &from {
                FilesFor::Chat(chat) => self.client.attachment_remove(chat, &file),
                FilesFor::NewWorkspace => self.client.new_workspace_draft_file_remove(&file),
            };
            match removed {
                // Let go of, or the hub keeps it, such as for a prompt that carries it.
                Ok(()) | Err(ClientError::Api(_) | ClientError::Unexpected(_)) => {}
                Err(
                    ClientError::Io(_)
                    | ClientError::Disconnected
                    | ClientError::FingerprintMismatch { .. }
                    | ClientError::Unreachable { .. },
                ) => {
                    left.push((from, file));
                    self.stalled = true;
                }
            }
        }
        self.removing = left;
    }

    /// Counts down one of the changes or files for a draft, which has reached the hub, or never needs to, along with
    /// the copy the hub gave back if it kept a change.
    fn reached(&self, key: &DraftKey, kept: Option<Agreed>) {
        let mut counts = self.counts.lock();
        if let Some(kept) = kept {
            counts.overruled.remove(key);
            agree(&mut counts, key.clone(), kept);
        }
        if let Some(count) = counts.unkept.get_mut(key) {
            *count -= 1;
            if *count == 0 {
                counts.unkept.remove(key);
                counts.on_its_way.remove(key);
                // Full only while a signal is waiting, which says the same.
                let _ = self.catching_up.try_send(());
            }
        }
    }
}
