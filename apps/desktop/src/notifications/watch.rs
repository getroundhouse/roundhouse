use crate::sidebar::signal::waiting_on_you;
use protocol::model::{Seq, Session, SessionId, Title, WorkspaceId};
use std::collections::{HashMap, HashSet};

/// What the app is known by to the system's notifications: the identifier its Mac bundle has, which Windows takes as
/// the app's own for its notifications too. A Mac reads it from the bundle, so it's for the other systems.
pub(crate) const APP_ID: &str = "dev.getroundhouse.desktop";

/// Why a chat needs the person, the most pressing first, which is the one said when a chat comes to several at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Why {
    WaitingOnYou,
    StoppedShort,
    Finished,
}

impl Why {
    /// What a notification says of it.
    pub(crate) fn said(self) -> &'static str {
        match self {
            Why::WaitingOnYou => "Waiting on you",
            Why::StoppedShort => "Stopped short",
            Why::Finished => "Finished",
        }
    }
}

/// A chat that has come to need the person, as a notification tells of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Came {
    pub(crate) chat: SessionId,
    pub(crate) workspace: WorkspaceId,
    pub(crate) title: Option<Title>,
    pub(crate) why: Why,
}

/// What a look found changed since the last.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Changes {
    /// Each chat that has come to need the person.
    pub(crate) came: Vec<Came>,
    /// Chats told of before that no longer need the person: read, answered, going again after stopping short, closed,
    /// gone, or being read.
    pub(crate) went: Vec<SessionId>,
}

/// What's known of a chat from the last look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Seen {
    pub(super) reply: Seq,
    pub(super) waiting: bool,
    pub(super) stopped: bool,
}

impl Seen {
    pub(super) fn of(session: &Session) -> Self {
        Self { reply: session.reply_seq, waiting: waiting_on_you([session]), stopped: session.stopped_short() }
    }
}

/// Follows every chat from one look to the next, for as long as the app is open, across connections, so what
/// happened while the hub restarted is told of once the records are back.
#[derive(Debug, Default)]
pub(crate) struct Watch {
    /// Each open chat as last seen.
    pub(super) seen: HashMap<SessionId, Seen>,
    /// The chats told of, whose notification may still show.
    pub(super) told: HashSet<SessionId>,
}

impl Watch {
    /// Looks at every chat there is, with `reading` the one being read, if any. A chat not seen before only takes
    /// note, since nothing says what it was before, so the first look, as the app opens, tells of nothing that was
    /// already waiting.
    pub(crate) fn look<'a>(
        &mut self,
        sessions: impl IntoIterator<Item = &'a Session>,
        reading: Option<&SessionId>,
    ) -> Changes {
        let open: Vec<&Session> = sessions.into_iter().filter(|s| s.is_open()).collect();
        let came: Vec<Came> = open
            .iter()
            .filter(|s| Some(&s.id) != reading)
            .filter_map(|s| Some((s, came_to_need(*self.seen.get(&s.id)?, s)?)))
            .map(|(s, why)| Came { chat: s.id.clone(), workspace: s.workspace_id.clone(), title: s.title.clone(), why })
            .collect();
        let still: HashSet<&SessionId> =
            open.iter().filter(|s| Some(&s.id) != reading && needs_the_person(s)).map(|s| &s.id).collect();
        let went: Vec<SessionId> = self.told.iter().filter(|id| !still.contains(id)).cloned().collect();
        for id in &went {
            self.told.remove(id);
        }
        self.told.extend(came.iter().map(|came| came.chat.clone()));
        self.seen = open.iter().map(|s| (s.id.clone(), Seen::of(s))).collect();
        Changes { came, went }
    }
}

/// Why a chat needs the person now when it didn't at the last look, if it does.
fn came_to_need(before: Seen, session: &Session) -> Option<Why> {
    let now = Seen::of(session);
    if now.waiting && !before.waiting {
        Some(Why::WaitingOnYou)
    } else if now.stopped && (!before.stopped || now.reply > before.reply) {
        // A turn that failed again, after one that failed before it, which a look between them didn't see start.
        Some(Why::StoppedShort)
    } else if now.reply > before.reply && session.unread() {
        Some(Why::Finished)
    } else {
        None
    }
}

/// Whether a chat still needs the person, so what told of it stands: while it needs a look, as the sidebar shows, with
/// a reply nobody has read, even while its agent works again, or waiting on them, and while it's stopped short.
fn needs_the_person(session: &Session) -> bool {
    let now = Seen::of(session);
    now.waiting || now.stopped || session.unread()
}
