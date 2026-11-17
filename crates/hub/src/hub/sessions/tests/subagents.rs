//! What the hub keeps of each subagent, and tells the feed of it, on a hub whose workspaces have stand-ins for their
//! workers, whose agents say what a test has them say. Reading a subagent's start, its work and its end from the CLI's
//! lines is its adapter's, as `agents`' `claude::normalize::subagents::tests` say, and what each event does to the
//! record is the store's, as `store::subagents::tests` say. The daemon's `e2e/subagents.rs` has a real adapter do it.

use protocol::capability::calls::call::Read;
use protocol::capability::calls::{Call, CallOutput};
use protocol::client::{ErrorCode, HubNotice};
use protocol::event::{ContentBlock, EventBody, TaskOutcome};
use protocol::fixtures::{typed_call, typed_result};
use protocol::model::{ModelName, Subagent, SubagentId, SubagentStatus};
use protocol::prompt::Prompt;

use crate::hub::stand_in_workers::{a_chat, a_turn, hub_with_stand_in_workers, the_agent_says};

/// What the subagent the agent names `task_c1`, which the call `c1` started, says as it starts, reads a file, hears
/// back, is counted, says what it found, and ends.
fn its_work() -> Vec<EventBody> {
    let subagent = || Some(SubagentId::from("task_c1"));
    let says = |model: Option<&str>, blocks| EventBody::Assistant {
        message_id: None,
        subagent: subagent(),
        model: model.map(Into::into),
        blocks,
        first_index: None,
    };
    let read = Call::Read(Read { path: "README.md".into(), lines: None, pages: None });
    let found = "There's a README here.";
    vec![
        EventBody::SubagentStarted {
            call: "c1".into(),
            subagent: subagent(),
            kind: Some("Explore".into()),
            description: None,
            prompt: Some("Look around.".into()),
            in_background: true,
        },
        says(Some("claude-haiku-5-5"), vec![typed_call("read", read)]),
        EventBody::AgentUser {
            subagent: subagent(),
            blocks: vec![typed_result("read", CallOutput::Words { text: "# Hello".into() }, false)],
        },
        EventBody::SubagentContext { subagent: "task_c1".into(), tokens: 1_200 },
        says(None, vec![ContentBlock::Text { text: found.into(), images: Vec::new() }]),
        EventBody::SubagentEnded {
            subagent: "task_c1".into(),
            outcome: TaskOutcome::Finished,
            report: Some(found.into()),
            context_tokens: Some(1_500),
        },
    ]
}

/// Each change to a subagent's record is told of on the feed as the record is then, newer each time, and the last is
/// what the chat's list of its subagents says. So a client shows each subagent at work without reading its events.
#[tokio::test]
async fn the_feed_tells_of_each_change_to_a_subagents_record_and_the_list_says_the_last() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    a_turn(&hub, &asked, &chat.id, Prompt::new("look around"), None).await;
    let mut feed = hub.subscribe_feed();

    the_agent_says(&hub, &asked, &chat.id, its_work()).await;

    let notices: Vec<Subagent> = std::iter::from_fn(|| feed.try_recv().ok())
        .filter_map(|notice| if let HubNotice::Subagent(subagent) = notice { Some(subagent) } else { None })
        .collect();
    // Each copy's calls, whether its newest call had heard back, whether it had said anything since, how many tokens
    // its conversation held, and its model.
    let heard: Vec<_> = notices
        .iter()
        .map(|subagent| {
            let lately = subagent.lately.as_ref();
            let answered = lately.and_then(|work| work.calls.last()).is_some_and(|made| made.result.is_some());
            let since = lately.is_some_and(|work| work.since.is_some());
            let model = subagent.model.as_ref().map(ModelName::as_str);
            (subagent.status, subagent.calls, answered, since, subagent.context_tokens, model)
        })
        .collect();
    let (working, finished, haiku) = (SubagentStatus::Working, SubagentStatus::Finished, Some("claude-haiku-5-5"));
    let told = [
        (working, 0, false, false, None, None),
        (working, 1, false, false, None, haiku),
        (working, 1, true, false, None, haiku),
        (working, 1, true, false, Some(1_200), haiku),
        (working, 1, true, true, Some(1_200), haiku),
        (finished, 1, false, false, Some(1_500), haiku),
    ];
    let as_it = "as it started, made its call, heard back, was counted, said what it found, and ended";
    assert_eq!(heard, told, "{as_it}");
    assert!(notices.windows(2).all(|pair| pair[0].rev < pair[1].rev), "each copy newer than the last");
    let listed = hub.session_subagents(chat.id.clone()).await.unwrap();
    let [subagent] = listed.subagents.as_slice() else { panic!("one subagent: {:?}", listed.subagents) };
    assert_eq!(notices.last(), Some(subagent), "the last told of is what's listed");
    assert_eq!((subagent.id(), subagent.call.as_str()), (SubagentId::from("task_c1"), "c1"));
    assert_eq!((subagent.prompt.as_deref(), subagent.in_background), (Some("Look around."), true));
    assert_eq!(subagent.report.as_deref(), Some("There's a README here."));
    assert!(subagent.rev <= listed.rev, "as of the list's revision");
    let missing = hub.session_subagents("missing".into()).await.map_err(|e| e.code);
    assert_eq!(missing.err(), Some(ErrorCode::NotFound));
}
