//! Moving a chat to another account the hub holds a login for, and what that does to its agent and its next launch.

use protocol::capability::conversation::Start;
use protocol::client::ErrorCode;
use protocol::event::{EventBody, ExitReason};
use protocol::prompt::Prompt;
use protocol::worker::WorkerRequest;

use super::two_accounts::{TwoAccounts, cold};
use crate::hub::stand_in_workers::{TIMEOUT, the_agent_says};

/// The conversation each launch of the chat's agent was started on, and whether it was to carry it on.
fn started_on(accounts: &TwoAccounts) -> Vec<Start> {
    accounts.chats.asked.launches(&accounts.chat).into_iter().map(|spec| spec.start).collect()
}

/// How many times a stand-in was asked to stop the chat's agent.
fn stops(accounts: &TwoAccounts) -> usize {
    let stop = |request: &WorkerRequest| {
        let WorkerRequest::AgentStop(stop) = request else { return None };
        (stop.session_id == accounts.chat).then_some(())
    };
    accounts.chats.asked.requests(stop).len()
}

#[tokio::test]
async fn a_moved_chat_resumes_under_its_account_and_goes_back_once_moved_back() {
    let dir = tempfile::tempdir().unwrap();
    let accounts = TwoAccounts::start(dir.path()).await;
    let (chats, chat) = (&accounts.chats, &accounts.chat);

    let moved = chats.assign(chat, &accounts.personal.id, false).await.unwrap();
    let under_personal = chats.token_in_turn(chat, None).await;
    let back = chats.assign(chat, &accounts.work.id, false).await.unwrap();
    let under_work = chats.token_in_turn(chat, None).await;

    assert_eq!(moved.hub_account(), Some(&accounts.personal.id));
    assert_eq!(moved.account.as_ref(), Some(&accounts.personal.id), "the record names the account it goes on under");
    assert_eq!(under_personal.as_deref(), Some("personal-token"));
    assert_eq!((back.hub_account(), back.account.as_ref()), (Some(&accounts.work.id), Some(&accounts.work.id)));
    assert_eq!(under_work.as_deref(), Some("work-token"));
    assert_eq!(stops(&accounts), 2, "each move stopped the agent under the other account");
    assert_eq!(chats.launches(chat), [false, true, true], "each move resumed it");
    let opened = chats.log(chat).into_iter().find_map(|e| {
        let EventBody::AgentInit { agent_session_id, .. } = e.body else { return None };
        agent_session_id
    });
    let carried_on = Start::Resume { conversation: opened.expect("its first launch opened a conversation") };
    assert_eq!(started_on(&accounts)[1..], [carried_on.clone(), carried_on], "it carried on with that one");
}

#[tokio::test]
async fn a_chat_moved_to_the_account_its_on_keeps_its_agent() {
    let dir = tempfile::tempdir().unwrap();
    let accounts = TwoAccounts::start(dir.path()).await;
    let (chats, chat) = (&accounts.chats, &accounts.chat);

    let moved = chats.assign(chat, &accounts.work.id, false).await.unwrap();
    let kept = chats.token_in_turn(chat, None).await;

    assert_eq!((moved.hub_account(), moved.account.as_ref()), (Some(&accounts.work.id), Some(&accounts.work.id)));
    assert_eq!(kept.as_deref(), Some("work-token"));
    assert_eq!(stops(&accounts), 0, "nothing stopped it");
    assert_eq!(chats.launches(chat), [false], "nothing relaunched it");
}

/// The agent the move stops is at work, so the move waits for its exit, which comes once its worker has stopped it.
#[tokio::test(flavor = "multi_thread")]
async fn moving_a_chat_whose_agent_is_at_work_needs_force_and_leaves_it_stopped_short() {
    let dir = tempfile::tempdir().unwrap();
    let accounts = TwoAccounts::start(dir.path()).await;
    let (chats, chat) = (&accounts.chats, &accounts.chat);
    let (seq, _) = chats.hub.session_prompt(chat.clone(), None, Prompt::new("work"), None).await.unwrap();
    the_agent_says(&chats.hub, &chats.asked, chat, [EventBody::PromptTaken { prompt: seq }]).await;

    // A hub that took the move without being told to stop the agent would wait for the agent's exit, which nothing
    // here says, so the refusal has a limit.
    let refusing = chats.assign(chat, &accounts.personal.id, false);
    let refused =
        tokio::time::timeout(TIMEOUT, refusing).await.expect("the move waited to stop the agent").unwrap_err();
    let unmoved = chats.record(chat);
    let moving = {
        let (hub, chat, personal) = (chats.hub.clone(), chat.clone(), accounts.personal.id.clone());
        let asked = protocol::client::sessions::SessionAssignAccount { id: chat, account: personal, force: true };
        tokio::spawn(async move { hub.session_assign_account(asked).await })
    };
    waiting::task::until(TIMEOUT, "the agent to be stopped", || stops(&accounts) == 1).await;
    let exited =
        EventBody::AgentExited { reason: ExitReason::Requested, code: None, signal: Some(15), stderr_tail: vec![] };
    the_agent_says(&chats.hub, &chats.asked, chat, [exited]).await;
    let moved = moving.await.unwrap().unwrap();
    let stopped_short = chats.record(chat).stopped_short();
    let under = chats.token_in_turn(chat, None).await;

    assert_eq!(refused.code, ErrorCode::NeedsForce, "{refused}");
    let work = Some(&accounts.work.id);
    assert_eq!((unmoved.hub_account(), unmoved.account.as_ref()), (work, work), "nothing changed");
    let personal = Some(&accounts.personal.id);
    assert_eq!((moved.hub_account(), moved.account.as_ref()), (personal, personal));
    assert!(stopped_short, "{:?}", chats.record(chat).status);
    assert_eq!(under.as_deref(), Some("personal-token"));
    assert_eq!(chats.launches(chat), [false, true]);
}

/// Another account's cache holds none of the conversation, so the first request under it writes the conversation
/// again, as one does once the cache has expired. A move isn't taken for that.
#[tokio::test]
async fn the_first_request_after_a_move_isnt_taken_for_one_that_found_the_cache_expired() {
    let dir = tempfile::tempdir().unwrap();
    let accounts = TwoAccounts::start(dir.path()).await;
    let (chats, chat) = (&accounts.chats, &accounts.chat);

    let moved = chats.assign(chat, &accounts.personal.id, false).await.unwrap();
    let after_the_move = chats.token_in_turn(chat, Some(cold())).await;

    assert_eq!(moved.prompt_cache, None, "the move forgot the cache");
    assert_eq!(after_the_move.as_deref(), Some("personal-token"));
    assert_eq!(chats.found_expired(chat), []);
}
