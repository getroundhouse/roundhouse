// Against a stand-in hub on a Unix socket.
#![cfg(unix)]

use std::path::Path;
use std::time::Duration;

use protocol::client::new_workspace::NewWorkspaceMade;
use protocol::client::sessions::Sessions;
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, ReplyOutcome, admin};
use protocol::model::{AgentDescription, AgentKind, NewWorkspaceDraftToKeep, Rev, Session, Title, Workspace};

use super::*;
use crate::Client;
use crate::stand_in_hub::{self, Does};

const TIMEOUT: Duration = Duration::from_secs(10);

fn claude() -> AgentDescription {
    protocol::fixtures::claude()
}

/// Connects to a stand-in hub on `socket` with one workspace, listed at revision 1, which answers asking for its
/// agents with `agents`, and each request `answers` has an answer for with that.
fn stand_in(
    socket: &Path,
    agents: ReplyOutcome,
    mut answers: impl FnMut(&ClientRequest) -> Option<ReplyOutcome> + Send + 'static,
) -> Client {
    stand_in_hub::connect(socket, move |request| {
        let answer = if matches!(request, ClientRequest::FeedSubscribe(_)) {
            ReplyOutcome::Response(ClientResponse::Ok)
        } else if request == ClientRequest::AgentList {
            agents.clone()
        } else if request == ClientRequest::RecordsList {
            let workspaces = vec![protocol::fixtures::workspace("w")];
            ReplyOutcome::Response(ClientResponse::Records(Records {
                workspaces,
                rev: Rev::new(1),
                ..Records::default()
            }))
        } else if let Some(answer) = answers(&request) {
            answer
        } else {
            return Does::Nothing;
        };
        Does::Answers(Box::new(answer))
    })
}

/// Follows a stand-in hub on `socket`, as [`stand_in`] says, and waits until the copy has caught up.
fn following_on(
    socket: &Path,
    agents: ReplyOutcome,
    answers: impl FnMut(&ClientRequest) -> Option<ReplyOutcome> + Send + 'static,
) -> (Client, RecordsFeed) {
    let client = stand_in(socket, agents, answers);
    let feed = RecordsFeed::start(&client);
    waiting::thread::until(TIMEOUT, "the copy never caught up", || feed.connected());
    (client, feed)
}

/// The same, answering nothing else.
fn following(dir: &Path, agents: ReplyOutcome) -> RecordsFeed {
    following_on(&dir.join("hub.sock"), agents, |_| None).1
}

fn no_agents() -> ReplyOutcome {
    ReplyOutcome::Response(ClientResponse::Agents(admin::Agents { agents: Vec::new() }))
}

/// The workspace the stand-in lists, renamed since.
fn renamed() -> Workspace {
    Workspace { title: Some(Title::typed("Renamed").unwrap()), rev: Rev::new(2), ..protocol::fixtures::workspace("w") }
}

#[test]
fn the_copy_holds_what_the_hubs_agents_offer_beside_its_records() {
    let dir = tempfile::tempdir().unwrap();

    let feed =
        following(dir.path(), ReplyOutcome::Response(ClientResponse::Agents(admin::Agents { agents: vec![claude()] })));

    let shown = feed.snapshot();
    assert_eq!(shown.agents().of(AgentKind::Claude), Some(&claude()));
    assert_eq!(shown.workspaces().len(), 1);
}

#[test]
fn a_hub_too_old_to_describe_its_agents_still_has_its_records_followed() {
    let dir = tempfile::tempdir().unwrap();
    let refused = ReplyOutcome::Error(ApiError::new(ErrorCode::Unsupported, "an older hub"));

    let feed = following(dir.path(), refused);

    let shown = feed.snapshot();
    assert_eq!(*shown.agents(), Agents::default());
    assert_eq!(shown.workspaces().len(), 1);
}

/// The stand-in tells of no change, so only the replies can bring them.
#[test]
fn the_copy_has_the_records_a_reply_carries_by_the_time_the_call_returns_but_not_those_a_list_does() {
    let dir = tempfile::tempdir().unwrap();
    let made = NewWorkspaceMade {
        workspace: Workspace { rev: Rev::new(3), ..protocol::fixtures::workspace("made") },
        session: Session { workspace_id: "made".into(), rev: Rev::new(3), ..protocol::fixtures::session("s") },
        unchosen: None,
        unsent: None,
    };
    let listed = Session { rev: Rev::new(4), ..protocol::fixtures::session("listed") };
    let answers = (made.clone(), listed.clone());
    let (client, feed) = following_on(&dir.path().join("hub.sock"), no_agents(), move |request| {
        let (made, listed) = answers.clone();
        let answer = if matches!(request, ClientRequest::WorkspaceRename(_)) {
            ClientResponse::Workspace(renamed())
        } else if matches!(request, ClientRequest::NewWorkspaceCreate(_)) {
            ClientResponse::NewWorkspaceMade(Box::new(made))
        } else if matches!(request, ClientRequest::SessionList(_)) {
            ClientResponse::Sessions(Sessions { sessions: vec![listed] })
        } else {
            return None;
        };
        Some(ReplyOutcome::Response(answer))
    });
    while feed.changed().try_recv().is_ok() {}

    client.workspace_rename(&"w".into(), "Renamed").unwrap();
    client.new_workspace_create(&NewWorkspaceDraftToKeep::default(), crate::new_request_id()).unwrap();
    client.session_list(&"w".into()).unwrap();

    let shown = feed.snapshot();
    assert_eq!(shown.workspace(&"w".into()), Some(&renamed()));
    assert_eq!(shown.workspace(&"made".into()), Some(&made.workspace));
    assert_eq!(shown.session(&"s".into()), Some(&made.session));
    assert_eq!(shown.session(&listed.id), None, "a list says what's gone, which only the feed's own can");
    assert!(feed.changed().try_recv().is_ok(), "readers are told");
}

#[test]
fn a_copy_carried_on_on_a_new_connection_takes_in_what_its_replies_carry() {
    let dir = tempfile::tempdir().unwrap();
    let (_, feed) = following_on(&dir.path().join("first.sock"), no_agents(), |_| None);
    let next = stand_in(&dir.path().join("next.sock"), no_agents(), |request| {
        let renaming = matches!(request, ClientRequest::WorkspaceRename(_));
        renaming.then(|| ReplyOutcome::Response(ClientResponse::Workspace(renamed())))
    });
    feed.reconnect(&next);
    waiting::thread::until(TIMEOUT, "the copy never caught up again", || feed.connected());

    next.workspace_rename(&"w".into(), "Renamed").unwrap();

    assert_eq!(feed.snapshot().workspace(&"w".into()), Some(&renamed()));
}
