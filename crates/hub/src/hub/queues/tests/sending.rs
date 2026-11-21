//! When the hub sends what's queued: at once to an agent that's idle, and otherwise once its turn has ended, one turn at
//! a time, unless the person stopped the turn, or sends one from the queue themselves.

use protocol::capability::Control;
use protocol::event::{EventBody, TurnOutcome};
use protocol::model::ClientDevice;
use protocol::prompt::Prompt;
use protocol::worker::{AgentControl, WorkerRequest};

use super::turns::{Chat, ids, sent_at, turn_ends};

/// Each queued prompt is a turn of its own, so the next waits until the agent has taken the one before and finished it.
#[tokio::test]
async fn a_prompt_queued_for_an_idle_agent_is_sent_at_once_and_the_rest_in_order_each_as_a_turn_once_it_has_finished() {
    let dir = tempfile::tempdir().unwrap();
    let chat = Chat::idle(dir.path()).await;
    chat.queue("q0", Prompt::new("straight away")).await.unwrap();
    let [straight_away] = chat.given(1).await[..] else { panic!("sent at once") };
    chat.says([EventBody::PromptTaken { prompt: straight_away }]).await;

    chat.queue("q1", Prompt::new("first queued")).await.unwrap();
    let queued = chat.queue("q2", Prompt::new("second queued")).await.unwrap().queued;
    chat.caught_up().await;
    let meanwhile = chat.asked.prompts_given(&chat.id);
    chat.finishes(None, TurnOutcome::Success).await;
    let first = chat.given(2).await[1];
    chat.finishes(Some(first), TurnOutcome::Success).await;
    let second = chat.given(3).await[2];
    chat.finishes(Some(second), TurnOutcome::Success).await;

    assert_eq!(ids(&queued), ["q1", "q2"]);
    assert_eq!(meanwhile, [straight_away], "neither while the agent works");
    let events = chat.log();
    assert_eq!(sent_at(&events, "q0"), Some(straight_away));
    let ends = turn_ends(&events);
    assert_eq!(ends.len(), 3, "each queued prompt is a turn of its own");
    assert_eq!((sent_at(&events, "q1"), sent_at(&events, "q2")), (Some(first), Some(second)));
    assert!(ends[0] < first && first < ends[1] && ends[1] < second, "{ends:?}, {first}, {second}");
    assert!(chat.record().queued.is_empty());
}

/// The prompt the person sends after the stop is the agent's next, and the queue waits for its turn to end.
#[tokio::test]
async fn a_stopped_turn_holds_the_queue_until_another_turn_ends() {
    let dir = tempfile::tempdir().unwrap();
    let chat = Chat::at_work(dir.path()).await;
    chat.queue("q1", Prompt::new("held")).await.unwrap();

    chat.hub.session_cancel(chat.id.clone()).await.unwrap();
    let cancel = |request: &WorkerRequest| {
        matches!(request, WorkerRequest::AgentControl(AgentControl { control: Control::Cancel, .. })).then_some(())
    };
    assert_eq!(chat.asked.requests(cancel).len(), 1, "the agent was asked to stop, and stops");
    chat.finishes(None, TurnOutcome::Cancelled).await;
    let (next, _) = chat.hub.session_prompt(chat.id.clone(), None, Prompt::new("next"), None).await.unwrap();
    assert_eq!(chat.given(2).await[1], next, "the person's, not the queue's");
    chat.finishes(Some(next), TurnOutcome::Success).await;
    let held = chat.given(3).await[2];

    let events = chat.log();
    let ends = turn_ends(&events);
    assert_eq!(sent_at(&events, "q1"), Some(held));
    assert!(
        next < ends[1] && ends[1] < held,
        "held past the stop, and sent after the next turn: {ends:?} {next} {held}"
    );
    assert!(events.iter().all(|e| !matches!(&e.body, EventBody::PromptDropped { .. })));
    assert!(!chat.record().queue_held);
}

/// Into the turn the agent is working on, which steers it, while what's ahead of it in the queue waits for that turn to
/// end.
#[tokio::test]
async fn a_queued_prompt_sent_now_steers_the_turn_and_the_rest_wait_their_turn() {
    let dir = tempfile::tempdir().unwrap();
    let chat = Chat::at_work(dir.path()).await;
    chat.queue("q1", Prompt::new("later")).await.unwrap();
    chat.queue("q2", Prompt::new("steered")).await.unwrap();

    let left = chat.hub.session_send_queued(chat.id.clone(), "q2".into()).await.unwrap().queued;
    let again = chat.hub.session_send_queued(chat.id.clone(), "q2".into()).await.unwrap().queued;

    assert_eq!(ids(&left), ["q1"]);
    assert_eq!(again, left, "sent already, so nothing changes");
    let steered = chat.given(2).await[1];
    assert_eq!(sent_at(&chat.log(), "q2"), Some(steered), "given to the agent as it works");
    chat.caught_up().await;
    assert_eq!(chat.asked.prompts_given(&chat.id).len(), 2, "and q1 isn't, while it does");
    chat.finishes(Some(steered), TurnOutcome::Success).await;
    let later = chat.given(3).await[2];
    let events = chat.log();
    let ends = turn_ends(&events);
    assert_eq!(sent_at(&events, "q1"), Some(later));
    assert!(steered < ends[0] && ends[0] < later, "into the running turn, and q1 after it: {ends:?} {steered} {later}");
}

/// Said out loud, on the device that has the conversation, sent at once or from the queue, so the agent's adapter can
/// tell it how to talk, and clients mark it.
#[tokio::test]
async fn a_spoken_prompt_is_recorded_and_given_to_the_agent_as_spoken_whether_sent_or_queued() {
    let dir = tempfile::tempdir().unwrap();
    let chat = Chat::idle(dir.path()).await;
    let said = Prompt::new("is it done?").said_out_loud();
    let studio = ClientDevice { id: "d1".into(), name: "Mac Studio".into() };
    chat.hub.session_conversation_start(chat.id.clone(), studio.clone()).await.unwrap();

    let (sent, _) = chat.hub.session_prompt(chat.id.clone(), None, said.clone(), Some(studio.clone())).await.unwrap();
    chat.given(1).await;
    chat.says([EventBody::PromptTaken { prompt: sent }]).await;
    chat.hub.session_queue(chat.id.clone(), "q1".into(), said, Some(&studio)).await.unwrap();
    chat.finishes(Some(sent), TurnOutcome::Success).await;
    let queued = chat.given(2).await[1];

    let given = |request: &WorkerRequest| {
        let WorkerRequest::AgentControl(asked) = request else { return None };
        let Control::Prompt { prompt, .. } = &asked.control else { return None };
        Some(prompt.spoken)
    };
    assert_eq!(chat.asked.requests(given), [true, true]);
    let recorded: Vec<_> = chat
        .log()
        .into_iter()
        .filter_map(|e| {
            let EventBody::UserPrompt { prompt, .. } = e.body else { return None };
            Some((e.seq, prompt.spoken))
        })
        .collect();
    assert_eq!(recorded, [(sent, true), (queued, true)]);
}

/// Said out loud and queued, then sent once the mode was off, it goes as typed, so it gets a written reply, since nobody
/// is listening for one.
#[tokio::test]
async fn a_prompt_said_out_loud_that_waited_until_the_mode_was_off_goes_as_typed() {
    let dir = tempfile::tempdir().unwrap();
    let chat = Chat::at_work(dir.path()).await;
    let studio = ClientDevice { id: "d1".into(), name: "Mac Studio".into() };
    chat.hub.session_conversation_start(chat.id.clone(), studio.clone()).await.unwrap();
    let said = Prompt::new("and then?").said_out_loud();
    chat.hub.session_queue(chat.id.clone(), "q1".into(), said, Some(&studio)).await.unwrap();

    chat.hub.session_conversation_end(chat.id.clone()).await.unwrap();
    chat.finishes(None, TurnOutcome::Success).await;
    let queued = chat.given(2).await[1];

    let given = |request: &WorkerRequest| {
        let WorkerRequest::AgentControl(asked) = request else { return None };
        let Control::Prompt { prompt, .. } = &asked.control else { return None };
        Some(prompt.spoken)
    };
    assert_eq!(chat.asked.requests(given), [false, false]);
    let recorded = chat.log().into_iter().find(|e| e.seq == queued).unwrap();
    let EventBody::UserPrompt { prompt, .. } = recorded.body else { panic!("{recorded:?}") };
    assert!(!prompt.spoken);
}

/// A prompt sent or queued again under its ID, once the mode it was said in has ended, gets what it got the first time,
/// rather than being refused as said out loud with the mode off.
#[tokio::test]
async fn a_spoken_prompt_sent_again_after_the_mode_ended_gets_what_it_got_the_first_time() {
    let dir = tempfile::tempdir().unwrap();
    let chat = Chat::at_work(dir.path()).await;
    let studio = ClientDevice { id: "d1".into(), name: "Mac Studio".into() };
    chat.hub.session_conversation_start(chat.id.clone(), studio.clone()).await.unwrap();
    let said = || Prompt::new("one more thing").said_out_loud();
    let send = || chat.hub.session_prompt(chat.id.clone(), Some("p1".into()), said(), Some(studio.clone()));
    let first = send().await.unwrap();
    chat.hub.session_queue(chat.id.clone(), "q1".into(), said(), Some(&studio)).await.unwrap();

    chat.hub.session_conversation_end(chat.id.clone()).await.unwrap();
    let again = send().await.unwrap();
    let queued_again = chat.hub.session_queue(chat.id.clone(), "q1".into(), said(), Some(&studio)).await.unwrap();

    assert_eq!(again, (first.0, true));
    assert_eq!(ids(&queued_again.queued), ["q1"]);
    chat.caught_up().await;
    assert_eq!(chat.asked.prompts_given(&chat.id), [chat.given(2).await[0], first.0], "and nothing again");
}
