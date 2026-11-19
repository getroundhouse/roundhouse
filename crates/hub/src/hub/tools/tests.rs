use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::ErrorCode;
use protocol::model::{DiffComment, DiffSide, Rev, Workspace};
use protocol::worker::{AskReply, DiffComments, ServeTools, ToolsServing, WorkerAsk, WorkerRequest, WorkerResponse};

use crate::hub::tests::hub_in;
use crate::store::Made;
use crate::worker_link::WorkerLink;

#[tokio::test]
async fn a_worker_is_refused_what_it_asks_for_a_chat_in_another_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let w = hub.store.get_workspace(&"w".into()).unwrap();
    let other = Workspace { id: "v".into(), name: "other".into(), instance_id: "v".into(), ..w };
    hub.store.insert_workspace(&other, &crate::store::WorkspacePlacement::local_checkout("/repo"), None).unwrap();

    let link = WorkerLink::unconnected();

    let refused = hub.answer_ask(&"v".into(), &link, "s".into(), WorkerAsk::PullRequest).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::NotFound, "{refused:?}");
}

/// What an agent reads with the tool is what the person sees on the diff, and nothing of another workspace's.
#[tokio::test]
async fn a_worker_asking_for_the_diffs_comments_hears_its_own_workspaces_oldest_first() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let w = hub.store.get_workspace(&"w".into()).unwrap();
    let other = Workspace { id: "v".into(), name: "other".into(), instance_id: "v".into(), ..w };
    hub.store.insert_workspace(&other, &crate::store::WorkspacePlacement::local_checkout("/repo"), None).unwrap();
    // Kept by the store, so each says when it was made, rather than two in the same millisecond going by their IDs.
    let add = |id: &str, workspace: &str, created_at_ms: i64| {
        let comment = DiffComment {
            id: id.into(),
            workspace_id: workspace.into(),
            path: "a.rs".into(),
            side: DiffSide::New,
            line: 3,
            text: format!("Comment {id}"),
            created_at_ms,
            rev: Rev::default(),
        };
        let Made::New(made) = hub.store.add_comment(&comment, None).unwrap().made else { panic!("{id} is new") };
        made
    };
    let second = add("c2", "w", 20);
    add("c3", "v", 15);
    let first = add("c1", "w", 10);
    let link = WorkerLink::unconnected();

    let answered = hub.answer_ask(&"w".into(), &link, "s".into(), WorkerAsk::DiffComments).await.unwrap();

    assert_eq!(answered, AskReply::DiffComments(DiffComments { comments: vec![first, second] }));
}

#[tokio::test]
async fn an_ask_from_a_newer_worker_is_refused_as_unsupported() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let link = WorkerLink::unconnected();

    let refused = hub.answer_ask(&"w".into(), &link, "s".into(), WorkerAsk::Unknown).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::Unsupported);
}

/// The ports a stand-in worker was asked to serve its tools on, in order.
type AskedFor = Arc<Mutex<Vec<Option<u16>>>>;

/// A stand-in worker that serves its tools on `port`, whatever it's asked for, and keeps the ports it's asked for.
fn serving_on(port: u16) -> (Arc<WorkerLink>, AskedFor) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let kept = asked.clone();
    let link = WorkerLink::answering(move |request| {
        let WorkerRequest::ServeTools(ServeTools { port: wanted }) = request else {
            return Some(Ok(WorkerResponse::Ok));
        };
        kept.lock().push(wanted);
        Some(Ok(WorkerResponse::ToolsServing(ToolsServing { port })))
    });
    (link, asked)
}

#[tokio::test]
async fn a_worker_is_asked_for_the_port_its_tools_were_last_on_and_a_session_keeps_its_token() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let (first, _) = serving_on(4100);
    let token = hub.tools_for(&"w".into(), &"s".into(), &first).await.unwrap();
    let (next, asked) = serving_on(4200);

    let again = hub.tools_for(&"w".into(), &"s".into(), &next).await.unwrap();
    hub.tools_for(&"w".into(), &"s".into(), &next).await.unwrap();

    assert_eq!(*asked.lock(), [Some(4100), Some(4200)], "the port it was on, then the one it's on since");
    assert_eq!(again, token);
}

#[tokio::test(start_paused = true)]
async fn agents_go_without_the_tools_of_a_worker_that_didnt_serve_them_and_it_isnt_asked_again() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let asked = Arc::new(Mutex::new(0));
    let counted = asked.clone();
    let link = WorkerLink::answering(move |_| {
        *counted.lock() += 1;
        None
    });

    let first = hub.tools_for(&"w".into(), &"s".into(), &link).await;
    let next = hub.tools_for(&"w".into(), &"s".into(), &link).await;

    assert_eq!((first, next), (None, None));
    assert_eq!(*asked.lock(), 1);
}
