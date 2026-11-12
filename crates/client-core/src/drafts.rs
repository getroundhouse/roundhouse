//! Keeping what's typed and attached in each chat's composer on the hub, so it's there after the app restarts, and in
//! every client. A client hands over each change as it's made, and a thread of its own keeps the newest of each chat,
//! one call at a time, so they reach the hub in the order they were made. Each file goes before any draft that names
//! it, since the hub leaves out a file it hasn't got, and one let go of goes after the draft that stopped naming it,
//! since the hub keeps any a draft names. What a hub can't take, because the connection went or it doesn't keep drafts,
//! waits for the next connection. Each chat's count of what hasn't reached the hub says what quitting would lose, and
//! quitting waits for it with [`Drafts::flushed`], which needs no other thread. The same count, and the revision the hub
//! gave back for the last draft kept, say whether a chat's draft on the hub was made somewhere else, such as in another
//! client, which [`Drafts::from_elsewhere`] says.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use protocol::model::{AttachmentId, NewDraft, Rev, Session, SessionId};

use crate::{Client, ClientError, FileToSend};

#[cfg(test)]
mod tests;

/// The longest a change waits to be kept while typing goes on, so typing a word isn't a call for each letter, and an
/// app that stops without quitting loses at most this much of it.
pub(crate) const PAUSE: Duration = Duration::from_millis(500);

/// Keeps drafts on the hub. A clone is another handle on the same thread, which ends once every handle has gone.
#[derive(Clone)]
pub struct Drafts {
    requests: flume::Sender<Request>,
    counts: Counts,
    caught_up: flume::Receiver<()>,
}

/// What the client and the thread share of each chat, under one lock, so a chat is never counted as having nothing on
/// its way before the revision its last draft was kept at is known.
#[derive(Default)]
struct Counted {
    /// How many of each chat's drafts and files haven't reached the hub, counted as each is handed over and as the hub
    /// takes it, or it's replaced by a newer draft. A chat with none has nothing in its composer that only this client
    /// has.
    unkept: HashMap<SessionId, usize>,
    /// The newest revision the hub gave the chat's record as it kept one of this client's drafts.
    kept: HashMap<SessionId, Rev>,
}

type Counts = Arc<Mutex<Counted>>;

enum Request {
    Changed { id: SessionId, draft: NewDraft, now: bool },
    Attach { id: SessionId, file: FileToSend },
    Detach { id: SessionId, file: AttachmentId },
    Reconnect(Client),
    Flush(flume::Sender<()>),
}

impl Drafts {
    /// Starts keeping drafts on `client`'s hub.
    pub fn start(client: &Client) -> Self {
        let (requests, received) = flume::unbounded();
        let (catching_up, caught_up) = flume::bounded(1);
        let counts = Counts::default();
        let keeper = Keeper {
            client: client.clone(),
            pending: HashMap::new(),
            adding: VecDeque::new(),
            removing: Vec::new(),
            stalled: false,
            answered: false,
            counts: counts.clone(),
            catching_up,
            flushing: Vec::new(),
        };
        // Without the thread, nothing is kept, and the count says so, since it only goes down as the hub takes each.
        let _ = std::thread::Builder::new().name("hub-drafts".to_string()).spawn(move || keeper.run(&received));
        Self { requests, counts, caught_up }
    }

    /// Carries on on a new connection, keeping first what the last one couldn't.
    pub fn reconnect(&self, client: &Client) {
        let _ = self.requests.send(Request::Reconnect(client.clone()));
    }

    /// What's typed in the chat now, kept within [`PAUSE`].
    pub fn typed(&self, id: &SessionId, draft: NewDraft) {
        self.handed(id);
        let _ = self.requests.send(Request::Changed { id: id.clone(), draft, now: false });
    }

    /// The same, kept at once, for a change nobody's still typing, such as the composer emptied by sending.
    pub fn set(&self, id: &SessionId, draft: NewDraft) {
        self.handed(id);
        let _ = self.requests.send(Request::Changed { id: id.clone(), draft, now: true });
    }

    /// A file for the chat's drafts to name, added to the chat before any draft given after this is kept.
    pub fn attach(&self, id: &SessionId, file: FileToSend) {
        self.handed(id);
        let _ = self.requests.send(Request::Attach { id: id.clone(), file });
    }

    /// Lets go of a file the chat's draft no longer names, once the draft given before this is kept.
    pub fn detach(&self, id: &SessionId, file: &AttachmentId) {
        let _ = self.requests.send(Request::Detach { id: id.clone(), file: file.clone() });
    }

    /// Keeps everything given so far at once, and hears once it's kept, or once that's as far as it can get, such as
    /// without a connection.
    pub fn flushed(&self) -> flume::Receiver<()> {
        let (done, flushed) = flume::bounded(1);
        if let Err(flume::SendError(Request::Flush(done))) = self.requests.send(Request::Flush(done)) {
            let _ = done.send(());
        }
        flushed
    }

    /// Whether something handed over for the chat, a draft or a file, hasn't reached the hub yet, and would be lost if
    /// the app stopped now.
    pub fn unkept(&self, id: &SessionId) -> bool {
        self.counts.lock().unkept.contains_key(id)
    }

    /// Whether the draft on the hub's `session` record was made somewhere else, such as in another client, so a client
    /// shows it in place of its own. Nothing handed over for the chat is still on its way, and the record is newer than
    /// any the hub gave back as it kept this client's drafts there. A record can be newer for something else, such as
    /// its status, and then its draft is this client's anyway. While one of this client's is on its way, that one goes
    /// on and replaces whatever was made elsewhere meanwhile, and [`Drafts::caught_up`] says once it has arrived.
    pub fn from_elsewhere(&self, session: &Session) -> bool {
        let counts = self.counts.lock();
        let kept = counts.kept.get(&session.id).copied().unwrap_or_default();
        !counts.unkept.contains_key(&session.id) && session.rev > kept
    }

    /// Says that a chat now has nothing on its way to the hub, so a draft made elsewhere that
    /// [`Drafts::from_elsewhere`] held back meanwhile can be shown. It holds one signal at a time, so signals nobody has
    /// looked at yet gather into one, and each goes to one receiver, so one reader listens.
    pub fn caught_up(&self) -> &flume::Receiver<()> {
        &self.caught_up
    }

    /// Counted before it's sent, so the thread can't take it, and count it down, first.
    fn handed(&self, id: &SessionId) {
        *self.counts.lock().unkept.entry(id.clone()).or_default() += 1;
    }
}

/// The thread's side.
struct Keeper {
    client: Client,
    /// The newest draft of each chat that isn't kept yet, and when to keep it.
    pending: HashMap<SessionId, Pending>,
    /// Files to add to their chats before any draft is kept, in the order they came.
    adding: VecDeque<(SessionId, FileToSend)>,
    /// Files to let go of, each once its chat has no draft waiting to be kept.
    removing: Vec<(SessionId, AttachmentId)>,
    /// The connection went, so nothing more is tried until the next.
    stalled: bool,
    /// The hub has answered a draft on this connection. Until it has, each is kept at once, so a hub that doesn't
    /// keep them is found out straight away.
    answered: bool,
    counts: Counts,
    /// Signals [`Drafts::caught_up`].
    catching_up: flume::Sender<()>,
    /// Waiting for everything given so far to be kept.
    flushing: Vec<flume::Sender<()>>,
}

struct Pending {
    draft: NewDraft,
    due: Instant,
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
            if self.stalled || (self.pending.is_empty() && self.adding.is_empty() && self.removing.is_empty()) {
                for done in self.flushing.drain(..) {
                    let _ = done.send(());
                }
            }
        }
    }

    fn take(&mut self, request: Request) {
        let now = Instant::now();
        match request {
            Request::Changed { id, draft, now: at_once } => {
                // A change while one waits keeps its time, so typing that goes on is still kept every so often.
                let waiting = self.pending.get(&id).map_or(now + PAUSE, |p| p.due);
                let due = if at_once || !self.answered { now } else { waiting };
                if self.pending.insert(id.clone(), Pending { draft, due }).is_some() {
                    // The one it replaces never needs to reach the hub.
                    self.reached(&id, None);
                }
            }
            Request::Attach { id, file } => self.adding.push_back((id, file)),
            Request::Detach { id, file } => self.removing.push((id, file)),
            Request::Reconnect(client) => {
                // It may be another hub, which may keep drafts where the last didn't.
                (self.client, self.stalled, self.answered) = (client, false, false);
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

    /// Adds every file waiting, then keeps each draft `due` picks, then lets go of the files whose chats have no draft
    /// left waiting, unless the connection goes.
    fn keep(&mut self, due: impl Fn(&Pending) -> bool) {
        self.add_files();
        self.keep_drafts(due);
        self.remove_files();
    }

    fn keep_drafts(&mut self, due: impl Fn(&Pending) -> bool) {
        let ids: Vec<SessionId> = self.pending.iter().filter(|(_, p)| due(p)).map(|(id, _)| id.clone()).collect();
        for id in ids {
            if self.stalled {
                return;
            }
            let Some(pending) = self.pending.remove(&id) else { continue };
            let answer = self.client.session_draft(&id, &pending.draft);
            // For the next connection, which may be to a hub that keeps drafts, such as this one once it's restarted as
            // the build that does.
            let held = answer
                .as_ref()
                .is_err_and(|e| e.is_unsupported() || matches!(e, ClientError::Io(_) | ClientError::Disconnected));
            if held {
                self.pending.insert(id, pending);
                self.stalled = true;
            } else {
                // Kept, or the hub answered and won't keep this one however often it's asked, such as for a chat
                // that's closed or gone.
                self.answered = true;
                self.reached(&id, answer.ok().map(|session| session.rev));
            }
        }
    }

    fn add_files(&mut self) {
        while !self.stalled
            && let Some((id, file)) = self.adding.pop_front()
        {
            let attachment = &file.attachment;
            // Such as a file of a prompt that came back because it couldn't be sent.
            if self.client.has_given(&id, &attachment.id) {
                self.reached(&id, None);
                continue;
            }
            let added = self.client.attachment_add(
                &id,
                &attachment.id,
                &attachment.name,
                attachment.media_type,
                file.data.clone(),
            );
            match added {
                // Added, or the hub won't take it however often it's asked, and then a draft goes without it.
                Ok(_) | Err(ClientError::Api(_) | ClientError::Unexpected(_)) => self.reached(&id, None),
                Err(ClientError::Io(_) | ClientError::Disconnected) => {
                    self.adding.push_front((id, file));
                    self.stalled = true;
                }
            }
        }
    }

    fn remove_files(&mut self) {
        let mut left = Vec::new();
        for (id, file) in std::mem::take(&mut self.removing) {
            if self.stalled || self.pending.contains_key(&id) {
                left.push((id, file));
                continue;
            }
            match self.client.attachment_remove(&id, &file) {
                // Let go of, or the hub keeps it, such as for a prompt that carries it.
                Ok(()) | Err(ClientError::Api(_) | ClientError::Unexpected(_)) => {}
                Err(ClientError::Io(_) | ClientError::Disconnected) => {
                    left.push((id, file));
                    self.stalled = true;
                }
            }
        }
        self.removing = left;
    }

    /// Counts down one of the chat's drafts or files, which has reached the hub, or never needs to, along with the
    /// revision the hub gave the chat's record if it kept a draft.
    fn reached(&self, id: &SessionId, kept: Option<Rev>) {
        let mut counts = self.counts.lock();
        if let Some(rev) = kept {
            let newest = counts.kept.entry(id.clone()).or_default();
            *newest = (*newest).max(rev);
        }
        if let Some(count) = counts.unkept.get_mut(id) {
            *count -= 1;
            if *count == 0 {
                counts.unkept.remove(id);
                // Full only while a signal is waiting, which says the same.
                let _ = self.catching_up.try_send(());
            }
        }
    }
}
