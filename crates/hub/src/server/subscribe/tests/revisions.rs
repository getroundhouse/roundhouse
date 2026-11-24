//! That every record a reply or a frame carries has a revision, as `crate::server::tests::revised` checks of every frame
//! a test's connection reads. This drives each request whose reply carries a record that the hub here can answer, on a
//! connection that follows each level, so what each sends reaches the connection as well.

use protocol::client::subscriptions::{Follow, Subscribe, Subscription};
use protocol::client::{
    CLIENT_PROTOCOL_VERSION, ClientRequest, ClientResponse, HubFrame, HubNotice, Outcome, Request, admin, comments,
    drafts, records, sessions, workspaces,
};
use protocol::model::{
    DiffSide, GithubRepositoryName, HubSettingsChange, IssueNumber, NewWorkspaceDraftToKeep, ReferenceAsk,
};

use super::given::{ByKind, Given, ok, start};
use crate::hub::tests::terminal_in_w;
use crate::server::tests::Connection;

/// Follows each level, each reading GitHub as observations, so a reference's level lists them.
fn follows() -> Subscribe {
    let reference = ReferenceAsk {
        repository: GithubRepositoryName::named("example/project"),
        number: IssueNumber::try_from(7).unwrap(),
    };
    let follows = [
        Follow::Hub,
        Follow::Workspace { id: "w".into() },
        Follow::Session { id: "s".into(), from: None },
        Follow::Terminal { id: "t".into() },
        Follow::Reference { reference },
    ];
    let starts = (1..).zip(follows).map(|(id, follow)| Subscription { ..start(id, follow) });
    Subscribe { stop: Vec::new(), start: starts.collect() }
}

/// Each request whose reply carries a record, or a list of them, that the hub here answers, but adding a comment, whose
/// edit names it.
fn asked() -> Vec<ClientRequest> {
    let settings = HubSettingsChange { keep_awake: Some(false), ..HubSettingsChange::default() };
    let draft = NewWorkspaceDraftToKeep { text: "Next".into(), ..NewWorkspaceDraftToKeep::default() };
    vec![
        records::RepoList.into_request(),
        workspaces::WorkspaceList { include_archived: true }.into_request(),
        workspaces::WorkspaceRename { id: "w".into(), title: "Renamed".into() }.into_request(),
        comments::CommentList { workspace_id: "w".into() }.into_request(),
        sessions::SessionGet { id: "s".into() }.into_request(),
        sessions::SessionList { workspace_id: "w".into() }.into_request(),
        sessions::SessionRename { id: "s".into(), title: "Named".into() }.into_request(),
        admin::HubConfigure { settings }.into_request(),
        drafts::NewWorkspaceDraftKeep { draft }.into_request(),
        terminal_in_w("t2").into_request(),
    ]
}

/// What each reply is, by name, so a request that stops carrying a record fails the test.
fn kind(reply: &ClientResponse) -> String {
    let named = format!("{reply:?}");
    named.split(['(', ' ']).next().unwrap_or_default().to_string()
}

#[tokio::test]
async fn every_record_a_reply_or_a_frame_carries_has_a_revision() {
    let dir = tempfile::tempdir().unwrap();
    let given = Given::new(dir.path()).await;
    let mut client = Connection::greeted(&given.hub, CLIENT_PROTOCOL_VERSION).await;
    ok(&mut client, 1, follows()).await;
    let comment = comments::CommentAdd {
        workspace_id: "w".into(),
        path: "a.rs".into(),
        side: DiffSide::New,
        line: 1,
        text: "why".into(),
        client_request_id: None,
    };
    let ClientResponse::Comment(comment) = ok(&mut client, 2, comment).await else { panic!("not a comment") };
    let edit = comments::CommentEdit { id: comment.id, text: "why not".into() }.into_request();
    let mut replies = Vec::new();

    for (id, request) in (3..).zip(std::iter::once(edit).chain(asked())) {
        match client.reply(id, &request).await {
            Outcome::Response(reply) => replies.push(kind(&reply)),
            Outcome::Error(error) => panic!("{request:?}: {error:?}"),
        }
    }
    given.changes().await;
    let told = |sent: &[HubFrame]| {
        let kinds = ByKind::of(sent);
        kinds.notices.iter().any(|notice| matches!(notice, HubNotice::Subagent(_))) && !kinds.pictures.is_empty()
    };
    client.until("each change", told).await;

    let carrying = [
        "Comment",
        "Repos",
        "Workspaces",
        "Workspace",
        "Comments",
        "Session",
        "Sessions",
        "Session",
        "HubSettings",
        "NewWorkspaceDraft",
        "Terminal",
    ];
    assert_eq!(replies, carrying, "each carries a record, or a list of them");
    let listed = client.sent.iter().filter(|frame| matches!(frame, HubFrame::Listed { .. })).count();
    assert_eq!(listed, 4, "the hub's, the workspace's, the session's and the reference's lists");
}
