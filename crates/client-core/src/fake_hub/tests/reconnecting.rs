//! What a client shows once it connects again, to a hub that may be another, or the same one with its revisions started
//! again, as after its database is replaced from a copy.

use std::time::Duration;

use protocol::model::{
    ChangeSummary, Ending, LineCounts, MessageId, Observation, Observed, ReplyHeard, ReplyPlace, Rev, SessionId,
    SessionStatus, SettledBy, WorkspaceId,
};

use super::super::World;
use super::served::served;
use crate::{OpenSession, RecordsFeed, ShownOfGithub, View};

const TIMEOUT: Duration = Duration::from_secs(10);

/// The hub `n`: the workspace `elm`, its chat, and issue 12, each saying `n` at every level that has a list, as a hub
/// whose newest revision is `rev`.
fn world(n: u32, rev: u64) -> (World, WorkspaceId, SessionId) {
    let mut world = World::default();
    let repo = world.repo("aspen");
    let elm = world.workspace(&repo, "elm");
    let chat = world.chat(&elm, SessionStatus::Idle, Vec::new());
    let rev = Rev::new(rev);
    let workspace = world.workspace_mut(&elm);
    workspace.name = format!("elm {n}");
    workspace.rev = rev;
    let lines = Observed::Current { value: LineCounts { added: n.into(), removed: 0 } };
    let changes = Observed::Current { value: ChangeSummary::new(format!("c{n}").into(), Vec::new()) };
    world.observations = vec![
        Observation::WorkspaceLines { workspace_id: elm.clone(), lines, rev },
        Observation::WorkspaceChanges { workspace_id: elm.clone(), changes, rev },
    ];
    let heard = ReplyHeard {
        session_id: chat.clone(),
        message: MessageId::from("msg-1"),
        ending: Ending::Stopped { through: ReplyPlace { block: 0, through: n } },
        settled_by: SettledBy::Device,
        rev,
    };
    world.replies_heard.insert(chat.clone(), vec![heard]);
    world.github_says.references.push(protocol::fixtures::issue("example/project", 12, &format!("Decide {n}")));
    (world, elm, chat)
}

/// What a view reads of each level that has a list: the hub's records and observations, the workspace's own, the
/// chat's own records, and a chip's.
#[derive(Debug, PartialEq)]
struct Read {
    name: Option<String>,
    lines: Observed<LineCounts>,
    base: Option<String>,
    heard: Vec<Ending>,
    chip: Option<String>,
}

impl Read {
    fn of(feed: &RecordsFeed, elm: &WorkspaceId, chat: &SessionId) -> Read {
        let snapshot = feed.snapshot();
        let base = match snapshot.workspace_changes(elm) {
            Some(Observed::Current { value } | Observed::AsOf { value, .. }) => Some(value.base_commit.to_string()),
            Some(Observed::NotKnown | Observed::Unknown) | None => None,
        };
        let chip = match snapshot.reference_said(&twelve()) {
            Observed::Current { value } | Observed::AsOf { value, .. } => {
                value.about().map(|about| about.title.clone())
            }
            Observed::NotKnown | Observed::Unknown => None,
        };
        Read {
            name: snapshot.workspace(elm).map(|workspace| workspace.name.clone()),
            lines: snapshot.workspace_lines(elm).clone(),
            base,
            heard: snapshot.replies_heard(chat).iter().map(|heard| heard.ending).collect(),
            chip,
        }
    }

    /// What the hub `n` of [`world`] says.
    fn said_by(n: u32) -> Read {
        Read {
            name: Some(format!("elm {n}")),
            lines: Observed::Current { value: LineCounts { added: n.into(), removed: 0 } },
            base: Some(format!("c{n}")),
            heard: vec![Ending::Stopped { through: ReplyPlace { block: 0, through: n } }],
            chip: Some(format!("Decide {n}")),
        }
    }
}

fn twelve() -> protocol::model::ReferenceAsk {
    protocol::fixtures::issue("example/project", 12, "").asked().unwrap()
}

/// A client that has taken in a hub's records at revision 50, at every level that has a list, connects to a hub whose
/// newest revision is 2, as the same hub is after its database is replaced from a copy, and shows what the second says
/// as soon as it lists each level, rather than once its revisions pass the first's.
#[test]
fn a_hub_whose_revisions_are_lower_is_shown_as_soon_as_it_lists_each_level() {
    let (first, elm, chat) = world(1, 50);
    let (second, ..) = world(2, 2);
    let (_first_dir, first) = served(first);
    let (_second_dir, second) = served(second);
    let feed = RecordsFeed::start(&first);
    waiting::thread::until(TIMEOUT, "the first hub's records", || feed.connected());
    let _followed = feed.follow_workspace(&elm);
    let session = feed.snapshot().session(&chat).unwrap().clone();
    let open = OpenSession::start(&first, &feed, &session, View::Own);
    let view = feed.github().view();
    view.show(ShownOfGithub { workspaces: Vec::new(), references: vec![twelve()] });
    let read = || Read::of(&feed, &elm, &chat);
    waiting::thread::until(TIMEOUT, "what the first hub says at every level", || read() == Read::said_by(1));

    feed.reconnect(&second);
    open.reconnect(&second);

    waiting::thread::until(TIMEOUT, "what the second hub says at every level", || read() == Read::said_by(2));
}
