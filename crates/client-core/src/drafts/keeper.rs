//! The thread that keeps what [`Drafts`](super::Drafts) is handed, one call at a time.

use std::collections::{HashMap, HashSet, VecDeque};
use std::mem::Discriminant;
use std::time::Instant;

use protocol::model::{AttachmentId, SessionId};

use super::{Agreed, Change, Counts, PAUSE, Request, Said, Where, agree};
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
        refused: HashSet::new(),
        answered: false,
        counts,
        catching_up,
        flushing: Vec::new(),
    };
    // Without the thread, nothing is kept, and the count says so, since it only goes down as the hub takes each.
    let _ = std::thread::Builder::new().name("hub-drafts".to_string()).spawn(move || keeper.run(&requests));
}

struct Keeper {
    client: Client,
    /// The newest change in each place that isn't kept yet, and when to keep it.
    pending: HashMap<Where, Pending>,
    /// Files to add to their chats before any draft is kept, in the order they came.
    adding: VecDeque<(SessionId, FileToSend)>,
    /// Files to let go of, each once its chat has no draft waiting to be kept.
    removing: Vec<(SessionId, AttachmentId)>,
    /// The connection went, or the hub doesn't keep chats' drafts, so nothing more is tried until the next.
    stalled: bool,
    /// The kinds of place this connection's hub doesn't keep, such as answers and comments on a hub that keeps only
    /// chats' drafts. What's typed there waits for the next connection, and the rest goes on.
    refused: HashSet<Discriminant<Where>>,
    /// The hub has answered a change on this connection. Until it has, each is kept at once, so a hub that doesn't
    /// keep them is found out straight away.
    answered: bool,
    counts: Counts,
    /// Signals [`Drafts::caught_up`](super::Drafts::caught_up).
    catching_up: flume::Sender<()>,
    /// Waiting for everything given so far to be kept.
    flushing: Vec<flume::Sender<()>>,
}

struct Pending {
    change: Change,
    due: Instant,
}

impl Keeper {
    fn run(mut self, requests: &flume::Receiver<Request>) {
        loop {
            let waiting = self.pending.iter().filter(|(at, _)| !self.refuses(at));
            let next = waiting.map(|(_, p)| p.due).min().filter(|_| !self.stalled);
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
            let held = self.pending.keys().all(|at| self.refuses(at));
            if self.stalled || (held && self.adding.is_empty() && self.removing.is_empty()) {
                for done in self.flushing.drain(..) {
                    let _ = done.send(());
                }
            }
        }
    }

    fn refuses(&self, at: &Where) -> bool {
        self.refused.contains(&std::mem::discriminant(at))
    }

    fn take(&mut self, request: Request) {
        let now = Instant::now();
        match request {
            Request::Changed { change, now: at_once } => {
                let at = change.at();
                // A change while one waits keeps its time, so typing that goes on is still kept every so often.
                let waiting = self.pending.get(&at).map_or(now + PAUSE, |p| p.due);
                let due = if at_once || !self.answered { now } else { waiting };
                if self.pending.insert(at.clone(), Pending { change, due }).is_some() {
                    // The one it replaces never needs to reach the hub.
                    self.reached(&at, None);
                }
            }
            Request::Attach { id, file } => self.adding.push_back((id, file)),
            Request::Detach { id, file } => self.removing.push((id, file)),
            Request::Reconnect(client) => {
                // It may be another hub, which may keep what the last didn't.
                (self.client, self.stalled, self.answered) = (client, false, false);
                self.refused.clear();
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

    /// Adds every file waiting, then keeps each change `due` picks, then lets go of the files whose chats have no
    /// draft left waiting, unless the connection goes.
    fn keep(&mut self, due: impl Fn(&Pending) -> bool) {
        self.add_files();
        self.keep_changes(due);
        self.remove_files();
    }

    fn keep_changes(&mut self, due: impl Fn(&Pending) -> bool) {
        let picked = self.pending.iter().filter(|(at, p)| due(p) && !self.refuses(at));
        let picked: Vec<Where> = picked.map(|(at, _)| at.clone()).collect();
        for at in picked {
            if self.stalled {
                return;
            }
            // Refused earlier in this pass, by a change in another place of the same kind.
            if self.refuses(&at) {
                continue;
            }
            let Some(pending) = self.pending.remove(&at) else { continue };
            let answer = self.send(&pending.change);
            let gone = matches!(answer, Err(ClientError::Io(_) | ClientError::Disconnected));
            let unsupported = answer.as_ref().is_err_and(ClientError::is_unsupported);
            // For the next connection, which may be to a hub that keeps it, such as this one once it's restarted as
            // the build that does. A hub that doesn't keep chats' drafts can't keep their files either, so everything
            // waits, but one that only doesn't keep answers or comments keeps the rest meanwhile.
            if gone || (unsupported && matches!(at, Where::Composer(_))) {
                self.pending.insert(at, pending);
                self.stalled = true;
            } else if unsupported {
                self.refused.insert(std::mem::discriminant(&at));
                self.pending.insert(at, pending);
            } else {
                // Kept, or the hub answered and won't keep this one however often it's asked, such as for a chat
                // that's closed or gone.
                self.answered = true;
                self.reached(&at, answer.ok());
            }
        }
    }

    /// Has the hub keep `change`, and says what the record it kept it on says is there now.
    fn send(&self, change: &Change) -> Result<Agreed, ClientError> {
        match change {
            Change::Composer(id, draft) => self
                .client
                .session_draft(id, draft)
                .map(|session| Agreed { rev: session.rev, said: Said::Composer(session.draft) }),
            Change::Answer(id, draft) => self
                .client
                .session_answer_draft(id, draft.as_ref())
                .map(|session| Agreed { rev: session.rev, said: Said::Answer(session.answer_draft) }),
            Change::Comment(id, draft) => self
                .client
                .workspace_comment_draft(id, draft.as_ref())
                .map(|workspace| Agreed { rev: workspace.rev, said: Said::Comment(workspace.comment_draft) }),
        }
    }

    fn add_files(&mut self) {
        while !self.stalled
            && let Some((id, file)) = self.adding.pop_front()
        {
            let attachment = &file.attachment;
            let at = Where::Composer(id.clone());
            // Such as a file of a prompt that came back because it couldn't be sent.
            if self.client.has_given(&id, &attachment.id) {
                self.reached(&at, None);
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
                Ok(_) | Err(ClientError::Api(_) | ClientError::Unexpected(_)) => self.reached(&at, None),
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
            if self.stalled || self.pending.contains_key(&Where::Composer(id.clone())) {
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

    /// Counts down one of the changes or files in a place, which has reached the hub, or never needs to, along with
    /// the copy the hub gave back if it kept a change.
    fn reached(&self, at: &Where, kept: Option<Agreed>) {
        let mut counts = self.counts.lock();
        if let Some(kept) = kept {
            agree(&mut counts, at.clone(), kept);
        }
        if let Some(count) = counts.unkept.get_mut(at) {
            *count -= 1;
            if *count == 0 {
                counts.unkept.remove(at);
                // Full only while a signal is waiting, which says the same.
                let _ = self.catching_up.try_send(());
            }
        }
    }
}
