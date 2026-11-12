use protocol::model::PullRequest;

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
    AskOutcome::Response(AskReply::PullRequest { pull_request: None::<PullRequest> })
}

#[tokio::test]
async fn an_answer_reaches_the_ask_it_names() {
    let (asks, mut sent) = asks();
    let first = asks.ask("s".into(), WorkerAsk::PullRequest);
    let second = asks.ask("t".into(), WorkerAsk::PullRequest);
    let answering = async {
        let (a, b) = (asked(&mut sent).await, asked(&mut sent).await);
        asks.answer(AskAnswer { answer: b, outcome: AskOutcome::Error(ApiError::new(ErrorCode::NotFound, "no")) });
        asks.answer(AskAnswer { answer: a, outcome: none() });
    };

    let (first, second, ()) = tokio::join!(first, second, answering);

    assert_eq!(first.unwrap(), AskReply::PullRequest { pull_request: None });
    assert_eq!(second.unwrap_err().code, ErrorCode::NotFound);
}

#[tokio::test]
async fn an_ask_fails_once_the_hub_has_gone_and_so_does_one_after() {
    let (asks, mut sent) = asks();
    let waiting = asks.ask("s".into(), WorkerAsk::PullRequest);
    let closing = async {
        asked(&mut sent).await;
        asks.close();
    };

    let (waiting, ()) = tokio::join!(waiting, closing);

    assert_eq!(waiting.unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(asks.ask("s".into(), WorkerAsk::PullRequest).await.unwrap_err().code, ErrorCode::Unavailable);
}

#[tokio::test]
async fn a_hub_that_never_answers_is_given_up_on() {
    let (frames, _sent) = mpsc::channel(8);
    let asks = Asks::waiting_at_most(Duration::from_millis(10), frames);

    let error = asks.ask("s".into(), WorkerAsk::PullRequest).await.unwrap_err();

    assert_eq!(error.code, ErrorCode::Unavailable);
    assert!(asks.waiting.lock().as_ref().is_some_and(HashMap::is_empty), "it isn't waited for any more");
}

#[tokio::test]
async fn an_ask_nobody_waits_for_any_more_is_forgotten() {
    let (asks, mut sent) = asks();
    let mut waiting = Box::pin(asks.ask("s".into(), WorkerAsk::PullRequest));
    tokio::select! {
        biased;
        _ = &mut waiting => panic!("nothing answered it"),
        id = asked(&mut sent) => assert_eq!(id, AskId::new(1)),
    }

    drop(waiting);

    assert!(asks.waiting.lock().as_ref().is_some_and(HashMap::is_empty));
}
