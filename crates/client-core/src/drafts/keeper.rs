//! The thread that keeps what [`Drafts`](super::Drafts) is handed, one call at a time.

use std::collections::{HashMap, HashSet, VecDeque};
use std::mem::Discriminant;
use std::time::Instant;

use protocol::model::AttachmentId;

use super::{Agreed, Change, Counts, FilesFor, PAUSE, Request, Said, Where, agree};
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
    /// Files to give the hub for what's typed where they're attached, a chat's composer or the new-workspace dialog,
    /// before any draft is kept, in the order they came.
    adding: VecDeque<(FilesFor, FileToSend)>,
    /// Files to let go of, each once its place has no draft waiting to be kept.
    removing: Vec<(FilesFor, AttachmentId)>,
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
            // Everything left waits for another connection.
            let mut files = self.adding.iter().map(|(to, _)| to).chain(self.removing.iter().map(|(from, _)| from));
            let held = self.pending.keys().all(|at| self.refuses(at)) && files.all(|f| self.refuses(&f.at()));
            if self.stalled || held {
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
            Request::Attach { to, file } => self.adding.push_back((to, file)),
            Request::Detach { from, file } => self.detach(from, file),
            Request::Reconnect(client) => {
                // It may be another hub, which may keep what the last didn't. The dialog's copy agreed on went as the
                // client was handed the connection, and goes again now, in case the last hub's answer came since.
                (self.client, self.stalled, self.answered) = (client, false, false);
                self.refused.clear();
                self.counts.lock().agreed.remove(&Where::NewWorkspace);
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

    /// Lets go of `file`, unless it's still waiting to be added, such as while the hub doesn't keep the dialog, when it
    /// never needs to reach the hub. Each file attached has an ID of its own, so it's the same one.
    fn detach(&mut self, from: FilesFor, file: AttachmentId) {
        let waiting = self.adding.iter().position(|(to, waiting)| *to == from && waiting.attachment.id == file);
        if let Some(i) = waiting {
            self.adding.remove(i);
            self.reached(&from.at(), None);
            // Unless the hub has it anyway, such as a file of a prompt that came back because it couldn't be sent.
            if !matches!(&from, FilesFor::Chat(id) if self.client.has_given(id, &file)) {
                return;
            }
        }
        self.removing.push((from, file));
    }

    /// Adds every file waiting, then keeps each change `due` picks, then lets go of the files whose places have no
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
            Change::NewWorkspace(draft) => self
                .client
                .new_workspace_draft_keep(draft)
                .map(|kept| Agreed { rev: kept.rev, said: Said::new_workspace(&kept) }),
        }
    }

    fn add_files(&mut self) {
        // Files for a place this hub doesn't keep, which wait for the next connection, in the order they came.
        let mut held = VecDeque::new();
        while !self.stalled
            && let Some((to, file)) = self.adding.pop_front()
        {
            let at = to.at();
            if self.refuses(&at) {
                held.push_back((to, file));
                continue;
            }
            match self.add(&to, &file) {
                // A hub that keeps chats' drafts but not the dialog's keeps neither its drafts nor its files.
                Err(e) if e.is_unsupported() && to == FilesFor::NewWorkspace => {
                    self.refused.insert(std::mem::discriminant(&at));
                    held.push_back((to, file));
                }
                // Added, or the hub won't take it however often it's asked, and then a draft goes without it.
                Ok(()) | Err(ClientError::Api(_) | ClientError::Unexpected(_)) => self.reached(&at, None),
                Err(ClientError::Io(_) | ClientError::Disconnected) => {
                    self.adding.push_front((to, file));
                    self.stalled = true;
                }
            }
        }
        // Held files came before any left, so they go back in front of them.
        for waiting in held.into_iter().rev() {
            self.adding.push_front(waiting);
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
            let at = from.at();
            if self.stalled || self.refuses(&at) || self.pending.contains_key(&at) {
                left.push((from, file));
                continue;
            }
            let removed = match &from {
                FilesFor::Chat(chat) => self.client.attachment_remove(chat, &file),
                FilesFor::NewWorkspace => self.client.new_workspace_draft_file_remove(&file),
            };
            match removed {
                // A hub that doesn't keep the dialog, such as an older one reached since, which the file waits for.
                Err(e) if e.is_unsupported() && from == FilesFor::NewWorkspace => {
                    self.refused.insert(std::mem::discriminant(&at));
                    left.push((from, file));
                }
                // Let go of, or the hub keeps it, such as for a prompt that carries it.
                Ok(()) | Err(ClientError::Api(_) | ClientError::Unexpected(_)) => {}
                Err(ClientError::Io(_) | ClientError::Disconnected) => {
                    left.push((from, file));
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
