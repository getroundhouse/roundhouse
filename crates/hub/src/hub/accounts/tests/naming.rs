//! Which account a chat names as its agent launches, as its worker reads the machine's login, and whose limits what
//! the agent hears are kept as.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use protocol::client::{ApiError, ErrorCode};
use protocol::event::{EventBody, ExitReason};
use protocol::model::Seq;

use super::on_a_machine::{a_turn_hearing, named, on_a_machine, the_week, without_when};
use crate::hub::stand_in_workers::{a_chat, hub_with_stand_in_workers, logged_in, logged_in_as, the_agent_says};

/// A chat names whose account its agent launched under once its worker says, before the agent has heard of any limit,
/// and chats in two workspaces whose machines are logged in to the same account name the one record, since a limit is
/// the account's, wherever it was heard.
#[tokio::test]
async fn a_chat_names_its_account_as_soon_as_its_agent_launches_and_one_login_in_two_workspaces_is_one_account() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let ada = logged_in_as("ada@example.com", "Max");
    let (first_workspace, first) = a_chat(&hub).await;
    let (second_workspace, second) = a_chat(&hub).await;
    on_a_machine(&hub, &first_workspace.id, &asked, ada.clone());
    on_a_machine(&hub, &second_workspace.id, &asked, ada.clone());

    let mut accounts = Vec::new();
    for chat in [&first, &second] {
        a_turn_hearing(&hub, &asked, &chat.id, Vec::new()).await;
        // Who it is is told once the session names it, so it's waited for too.
        accounts.push(named(&hub, &chat.id, |account| account.email.is_some()).await);
    }

    let [account, other] = &accounts[..] else { unreachable!() };
    assert_eq!((Some(&account.id), &account.email), (ada.id.as_ref(), &ada.email), "whose it is, from the launch");
    assert_eq!(account.plan, ada.plan);
    assert_eq!(account.limits, [], "and nothing heard of its limits");
    assert_eq!(other.id, account.id);
    assert_eq!(hub.store.list_records().unwrap().accounts.len(), 1);
}

#[tokio::test]
async fn a_chat_whose_agent_relaunches_under_another_login_names_that_account_and_the_first_keeps_its_limits() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    let ada = logged_in_as("ada@example.com", "Max");
    let machine = on_a_machine(&hub, &workspace.id, &asked, ada.clone());
    a_turn_hearing(&hub, &asked, &chat.id, vec![the_week()]).await;
    named(&hub, &chat.id, |account| without_when(&account.limits) == [the_week()]).await;
    let exited =
        EventBody::AgentExited { reason: ExitReason::Unexpected, code: Some(3), signal: None, stderr_tail: vec![] };
    the_agent_says(&hub, &asked, &chat.id, [exited]).await;

    // Someone else, with the same model provider, whose agent hears nothing of its limits.
    *machine.lock() = Ok(logged_in_as("grace@example.com", "Max"));
    a_turn_hearing(&hub, &asked, &chat.id, Vec::new()).await;
    let grace = named(&hub, &chat.id, |account| Some(&account.id) != ada.id.as_ref() && account.email.is_some()).await;

    assert_eq!((grace.email.as_deref(), grace.limits), (Some("grace@example.com"), vec![]), "Ada's limits aren't hers");
    let ada_now = hub.store.account(&ada.id.unwrap()).unwrap().unwrap();
    assert_eq!(without_when(&ada_now.limits), [the_week()]);
}

/// The worker couldn't say whose the machine's login is, twice, as when the CLI's call timed out, so the hub asks again
/// after a pause, and again, and what the agent heard meanwhile is kept under the account it then says. Once nothing
/// can say, what was heard is dropped, and the hub asks again only when the agent next hears of a limit, so this
/// agent's limits, heard once, are kept only by a hub that asked again by itself.
#[tokio::test(start_paused = true)]
async fn a_launchs_account_is_asked_again_when_the_worker_couldnt_say_and_what_its_agent_heard_meanwhile_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    let ada = logged_in_as("ada@example.com", "Max");
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    logged_in(&hub, &workspace.id, &asked, move |_| match counted.fetch_add(1, Ordering::SeqCst) {
        0 | 1 => Err(ApiError::new(ErrorCode::Worker, "the CLI didn't say who's logged in")),
        _ => Ok(logged_in_as("ada@example.com", "Max")),
    });

    // The week's limit is heard in the turn, before anyone knows whose it is.
    a_turn_hearing(&hub, &asked, &chat.id, vec![the_week()]).await;
    let account = named(&hub, &chat.id, |account| !account.limits.is_empty()).await;

    assert_eq!(Some(&account.id), ada.id.as_ref());
    assert_eq!(without_when(&account.limits), [the_week()]);
    assert_eq!(calls.load(Ordering::SeqCst), 3, "asked again until it could say");
    assert_eq!(hub.store.get_session(&chat.id).unwrap().session.account, ada.id);
}

/// A worker of a build from before its agents said their account's limits in a report says them in the log, which the
/// hub takes in as that report and doesn't keep, since the log no longer holds them.
#[tokio::test]
async fn limits_an_older_worker_says_in_the_log_are_kept_by_the_account_and_left_out_of_the_log() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    on_a_machine(&hub, &workspace.id, &asked, logged_in_as("ada@example.com", "Max"));
    a_turn_hearing(&hub, &asked, &chat.id, Vec::new()).await;

    the_agent_says(&hub, &asked, &chat.id, [EventBody::AgentLimits { limits: vec![the_week()] }]).await;

    named(&hub, &chat.id, |account| without_when(&account.limits) == [the_week()]).await;
    let log = hub.store.read_events(&chat.id, Seq::default(), u32::MAX, u64::MAX, false).unwrap();
    assert!(!log.iter().any(|event| matches!(event.body, EventBody::AgentLimits { .. })), "{log:?}");
}
