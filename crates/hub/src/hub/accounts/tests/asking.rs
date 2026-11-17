//! What a client from before accounts were records asks of a chat's account, which the hub answers from its worker.

use protocol::client::ErrorCode;
use protocol::worker::WorkerRequest;

use super::on_a_machine::{a_turn_hearing, named, on_a_machine, the_week, without_when};
use crate::hub::stand_in_workers::{a_chat, hub_with_stand_in_workers, logged_in_as};
use crate::store::now_ms;

#[tokio::test]
async fn a_chats_account_and_its_limits_can_still_be_asked_for_by_an_older_client() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    let ada = logged_in_as("ada@example.com", "Max");
    on_a_machine(&hub, &workspace.id, &asked, ada.clone());

    let before = hub.session_account(chat.id.clone()).await.unwrap();
    let asked_at_ms = now_ms();
    a_turn_hearing(&hub, &asked, &chat.id, vec![the_week()]).await;
    named(&hub, &chat.id, |account| without_when(&account.limits) == [the_week()]).await;
    let after = hub.session_account(chat.id.clone()).await.unwrap();

    assert_eq!((before.account, before.limits), (ada.clone(), Vec::new()), "no agent has heard of a limit yet");
    assert_eq!(after.account, ada);
    assert_eq!(without_when(&after.limits), [the_week()]);
    assert!(after.limits[0].heard_at_ms.is_some_and(|at_ms| at_ms >= asked_at_ms), "{:?}", after.limits);
}

/// Even once it names the account its agent last launched under, since only the worker says whose a login on its
/// machine is.
#[tokio::test]
async fn a_chat_in_an_archived_workspace_cant_be_asked_its_account_so_asking_starts_no_worker() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (workspace, chat) = a_chat(&hub).await;
    on_a_machine(&hub, &workspace.id, &asked, logged_in_as("ada@example.com", "Max"));
    a_turn_hearing(&hub, &asked, &chat.id, vec![the_week()]).await;
    named(&hub, &chat.id, |account| !account.limits.is_empty()).await;
    hub.workspace_archive(workspace.id.clone(), true).await.unwrap();
    let everything = |_: &WorkerRequest| Some(());
    let (asked_before, worker_before) =
        (asked.requests(everything).len(), hub.workers.lock().contains_key(&workspace.id));

    let refused = hub.session_account(chat.id.clone()).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::Conflict, "{refused:?}");
    assert_eq!(asked.requests(everything).len(), asked_before, "no worker was asked");
    assert_eq!(hub.workers.lock().contains_key(&workspace.id), worker_before, "nor started");
}
