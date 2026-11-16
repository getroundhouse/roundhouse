use super::super::Client;
use super::super::follow::{Follow, FollowError};
use super::search;
use super::source::SessionSource;
use crate::session_log::{By, Changes, RecordedApprovals, SessionLog};
use protocol::find::Needle;
use protocol::model::{ClientMessageId, Place, QueuedPrompt, Session, SessionId, SubagentId};
use search::{Search, Searched, Searches};
use std::ops::Range;
use std::sync::Arc;

/// Which of a session's events a view follows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum View {
    /// Every event, by `seq`.
    Whole,
    /// The agent's own events, by place. A hub from before places can't read them apart, so from one this is every
    /// event, by `seq`, which [`SessionLog::by`] says.
    Own,
    /// The events of one subagent, by place.
    Subagent(SubagentId),
}

impl View {
    /// The subagent whose events it follows, or `None` for the agent's own, or every event.
    pub(super) fn subagent(&self) -> Option<&SubagentId> {
        match self {
            View::Subagent(subagent) => Some(subagent),
            View::Whole | View::Own => None,
        }
    }
}

/// A session someone has open, kept current until every clone of this has gone. It fetches the history a
/// view is looking at, and carries on from a new connection given to [`OpenSession::reconnect`]. It holds the log as
/// versions, as [`OpenSession::version`] says.
#[derive(Clone)]
pub struct OpenSession {
    pub(super) follow: Follow<Arc<SessionLog>>,
    pub(super) session_id: SessionId,
    /// The view asked for, which is what a search looks through.
    pub(super) view: View,
    pub(super) searches: Arc<Searches>,
}

impl OpenSession {
    /// Starts following every event, on a thread of its own, so this doesn't wait for the hub. Once it has subscribed,
    /// it asks for the session's prompts.
    pub fn start(client: &Client, session: &Session) -> OpenSession {
        OpenSession::start_view(client, session, View::Whole)
    }

    /// Starts following the events `view` takes, on a thread of its own. Once it has subscribed, it asks for the
    /// session's prompts, unless it follows a subagent, whose events hold none.
    pub fn start_view(client: &Client, session: &Session, view: View) -> OpenSession {
        // One by place doesn't know how far it goes until the hub says.
        let mut log = match view {
            View::Whole => SessionLog::new(session.last_seq),
            View::Own | View::Subagent(_) => SessionLog::by_place(Place::default()),
        };
        // What waits is asked about wherever it was asked, from the start. A subagent's view asks nothing.
        if !matches!(view, View::Subagent(_)) {
            log.waiting_on(RecordedApprovals::of(session).as_ref());
        }
        let source = SessionSource {
            session_id: session.id.clone(),
            wanted: view.clone(),
            view: view.clone(),
            last_seq: session.last_seq,
            placed: false,
            read_from: client.hub_version().to_string(),
        };
        let follow = Follow::start(client, Arc::new(log), source);
        follow.wake();
        OpenSession { follow, session_id: session.id.clone(), view, searches: Arc::default() }
    }

    /// Says that something changed. It holds one signal at a time, so changes a view hasn't looked at yet
    /// gather into one, and a slow view never falls behind. Each signal goes to one receiver, so one view
    /// listens.
    pub fn changed(&self) -> &flume::Receiver<()> {
        &self.follow.changed
    }

    /// The log as it is now, which never changes: whatever comes next goes in the next version. So whatever reads one,
    /// such as a frame drawing each of its rows from it, reads the same log however much comes meanwhile, and keeps
    /// nothing waiting while it does. A version shares what it holds with the next, as a copy of a [`SessionLog`] does.
    pub fn version(&self) -> Arc<SessionLog> {
        self.follow.read(Arc::clone)
    }

    /// Reads the log as it is now, as [`OpenSession::version`] has it.
    pub fn read<T>(&self, read: impl FnOnce(&SessionLog) -> T) -> T {
        read(&self.version())
    }

    /// The log as it is now, as [`OpenSession::version`] has it, with what changed since a view last caught up, taken
    /// together so that what changed is what makes this version from the last one caught up with.
    pub fn catch_up(&self) -> (Arc<SessionLog>, Changes) {
        self.follow.quietly(|log| {
            let changes = Arc::make_mut(log).take_changes();
            (Arc::clone(log), changes)
        })
    }

    /// Keeps and fetches the whole range the viewport needs, including its buffer. Called even when all its rows
    /// are loaded, so a later fetch or live event cannot evict something still on screen. `unfetched` is the lowest
    /// row that frame could not draw; without one, only bodies made stale by a hub update need fetching.
    pub fn want(&self, rows: Range<usize>, unfetched: Option<usize>) {
        let (evicted, fetch) = self.follow.quietly(|log| {
            let evicted = !log.focused_on(&rows, unfetched) && Arc::make_mut(log).focus_on(rows, unfetched);
            (evicted, log.wants_pages())
        });
        if evicted {
            self.follow.teller().tell();
        }
        if fetch {
            self.follow.wake();
        }
    }

    /// Fetches each request `recorded` says is waiting that isn't loaded, as [`SessionLog::waiting_on`] says.
    pub fn waiting_on(&self, recorded: Option<&RecordedApprovals>) {
        self.follow.quietly(|log| Arc::make_mut(log).waiting_on(recorded));
        self.follow.wake();
    }

    /// Shows a prompt on its way, with what it carries, as [`crate::Written::on_its_way`] says.
    pub(crate) fn add_pending(&self, prompt: QueuedPrompt) {
        self.follow.update(|log| Arc::make_mut(log).add_pending(prompt));
    }

    /// Takes back a prompt that couldn't be sent.
    pub(crate) fn remove_pending(&self, client_msg_id: &ClientMessageId) -> Option<QueuedPrompt> {
        self.follow.update(|log| Arc::make_mut(log).remove_pending(client_msg_id))
    }

    pub(crate) fn session_id(&self) -> &SessionId {
        &self.session_id
    }

    /// Carries on on a new connection, from the last event it received.
    pub fn reconnect(&self, client: &Client) {
        self.searches.reconnected();
        self.follow.reconnect(client);
    }

    /// Looks through all of what the view follows for `needle`, which the log holds only a window of, on a thread of
    /// its own, and says when there's more through [`OpenSession::changed`]. The hub says which events hold it, by
    /// place, and the search reads each with what's within `around` places of it, a stretch at a time, for a view to
    /// count what the rows it doesn't hold draw. Events the log holds, and those it's given from now on, a view looks
    /// through itself. A log by `seq` is from a hub too old to read by place, and so to find. The search stops once it
    /// has gone, and `client` is the connection it asks on.
    pub fn search(&self, client: &Client, needle: Needle, around: usize) -> Search {
        let (every, by) = self.follow.read(|log| (log.holds_every_event(), log.by()));
        if every {
            return Search::finished(Searched::Done);
        }
        if needle.query().trim().chars().count() < Search::SHORTEST {
            return Search::finished(Searched::TooShort);
        }
        if by == By::Seq {
            return Search::finished(Searched::CantFind);
        }
        let asking = search::Asking { session_id: self.session_id.clone(), view: self.view.clone(), needle, around };
        search::Job::start(client, asking, &self.searches, self.follow.teller())
    }

    pub fn take_errors(&self) -> Vec<FollowError> {
        self.follow.take_errors()
    }
}
