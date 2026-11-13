use protocol::fixtures;
use protocol::model::WorkName;
use protocol::worker::{
    CreateWorktree, NewWorktreeOn, PullRequestAnswer, ReadPullRequest, ReadWait, RemoveWorktree, WorktreeCreated,
    WorktreeRemoved,
};

use super::*;

fn asks() -> (Asks, mpsc::Receiver<WorkerFrame>) {
    let (frames, sent) = mpsc::channel(8);
    (Asks::new(frames), sent)
}

/// What the hub was asked, by the ID it answers with.
async fn asked(sent: &mut mpsc::Receiver<WorkerFrame>) -> AskId {
    match sent.recv().await {
        Some(WorkerFrame::Ask { id, .. }) => id,
        other => panic!("expected an ask, got {other:?}"),
    }
}

fn none() -> AskOutcome {
    AskOutcome::Response(AskReply::PullRequest(PullRequestAnswer { pull_request: None }))
}

#[tokio::test]
async fn an_answer_reaches_the_ask_it_names() {
    let (asks, mut sent) = asks();
    let first = asks.ask("s".into(), ReadPullRequest);
    let second = asks.ask("t".into(), ReadPullRequest);
    let answering = async {
        let (a, b) = (asked(&mut sent).await, asked(&mut sent).await);
        asks.answer(AskAnswer { answer: b, outcome: AskOutcome::Error(ApiError::new(ErrorCode::NotFound, "no")) });
        asks.answer(AskAnswer { answer: a, outcome: none() });
    };

    let (first, second, ()) = tokio::join!(first, second, answering);

    assert_eq!(first.unwrap(), PullRequestAnswer { pull_request: None });
    assert_eq!(second.unwrap_err().code, ErrorCode::NotFound);
}

/// So a tool has only the answer it asked for to handle.
#[tokio::test]
async fn an_answer_of_another_kind_than_the_ask_names_fails_where_its_read() {
    let (asks, mut sent) = asks();
    let waiting = asks.ask("s".into(), ReadWait { wait: "w".into() });
    let answering = async {
        let id = asked(&mut sent).await;
        asks.answer(AskAnswer { answer: id, outcome: none() });
    };

    let (waiting, ()) = tokio::join!(waiting, answering);

    assert_eq!(waiting.unwrap_err().code, ErrorCode::Internal);
}

#[tokio::test]
async fn an_ask_fails_once_the_hub_has_gone_and_so_does_one_after() {
    let (asks, mut sent) = asks();
    let waiting = asks.ask("s".into(), ReadPullRequest);
    let closing = async {
        asked(&mut sent).await;
        asks.close();
    };

    let (waiting, ()) = tokio::join!(waiting, closing);

    assert_eq!(waiting.unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(asks.ask("s".into(), ReadPullRequest).await.unwrap_err().code, ErrorCode::Unavailable);
}

#[tokio::test]
async fn a_hub_that_never_answers_is_given_up_on() {
    let (frames, _sent) = mpsc::channel(8);
    let asks = Asks::waiting_at_most(Duration::from_millis(10), frames);

    let error = asks.ask("s".into(), ReadPullRequest).await.unwrap_err();

    assert_eq!(error.code, ErrorCode::Unavailable);
    assert!(asks.waiting.lock().as_ref().is_some_and(HashMap::is_empty), "it isn't waited for any more");
}

/// Making or removing a worktree can take minutes, so each is still waited for once two other asks, one after the
/// other, have each been given up on, which is twice as long as either is waited for.
#[tokio::test]
async fn an_ask_for_a_worktree_is_waited_for_longer_than_the_rest() {
    let (frames, mut sent) = mpsc::channel(8);
    let asks = Asks::waiting_at_most(Duration::from_millis(20), frames);
    let name = WorkName::try_from("x".to_string()).unwrap();
    let on = NewWorktreeOn::NewBranch { name, base: None };
    let made = || WorktreeCreated {
        worktree: fixtures::child_worktree("x"),
        commit: "abc".into(),
        copied: vec![],
        warnings: vec![],
    };
    let removed = || WorktreeRemoved { worktree: fixtures::child_worktree("x"), last: None };
    let making = asks.ask("s".into(), CreateWorktree { on, key: "k".into() });
    let removing = asks.ask("s".into(), RemoveWorktree { path: "/c".into(), force: false });
    let answering = async {
        let (adding, taking) = (asked(&mut sent).await, asked(&mut sent).await);
        let first = asks.ask("s".into(), ReadPullRequest).await;
        let second = asks.ask("s".into(), ReadPullRequest).await;
        asks.answer(AskAnswer { answer: adding, outcome: AskOutcome::Response(AskReply::WorktreeCreated(made())) });
        asks.answer(AskAnswer { answer: taking, outcome: AskOutcome::Response(AskReply::WorktreeRemoved(removed())) });
        (first, second)
    };

    let (made, removed, (first, second)) = tokio::join!(making, removing, answering);

    assert_eq!(first.unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(second.unwrap_err().code, ErrorCode::Unavailable);
    assert!(made.is_ok() && removed.is_ok(), "{made:?} {removed:?}");
}

#[tokio::test]
async fn an_ask_nobody_waits_for_any_more_is_forgotten() {
    let (asks, mut sent) = asks();
    let mut waiting = Box::pin(asks.ask("s".into(), ReadPullRequest));
    tokio::select! {
        biased;
        _ = &mut waiting => panic!("nothing answered it"),
        id = asked(&mut sent) => assert_eq!(id, AskId::new(1)),
    }

    drop(waiting);

    assert!(asks.waiting.lock().as_ref().is_some_and(HashMap::is_empty));
}

/// As when a hub goes, by restarting, before it has answered, or before it heard the ask at all.
#[tokio::test]
async fn an_ask_still_waiting_is_asked_again_and_only_its_first_answer_is_taken() {
    let (asks, mut sent) = asks();
    let waiting = asks.ask("s".into(), ReadPullRequest);
    let next_hub = async {
        let first = asked(&mut sent).await;
        asks.ask_again().await;
        let again = tokio::time::timeout(Duration::from_secs(5), asked(&mut sent)).await.expect("asked again");
        assert_eq!(again, first, "the same ask, by the same ID");
        asks.answer(AskAnswer { answer: again, outcome: none() });
        asks.answer(AskAnswer { answer: first, outcome: AskOutcome::Error(ApiError::new(ErrorCode::NotFound, "no")) });
    };

    let (waiting, ()) = tokio::join!(waiting, next_hub);

    assert_eq!(waiting.unwrap(), PullRequestAnswer { pull_request: None });
    asks.ask_again().await;
    assert!(sent.try_recv().is_err(), "nothing is left to ask once it's answered");
}
