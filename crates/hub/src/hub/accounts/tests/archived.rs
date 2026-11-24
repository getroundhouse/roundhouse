//! Asking whose account an agent runs under in a workspace that's archived, which only a worker can say.

use protocol::client::ErrorCode;
use protocol::worker::WorkerRequest;

use super::on_a_machine::{a_turn_hearing, named, on_a_machine, the_week};
use crate::hub::stand_in_workers::{a_chat, hub_with_stand_in_workers, logged_in_as};

/// A workspace that's archived can't be asked whose account its agents run under, even once a chat in it names the
/// account its agent last launched under, so asking starts no worker.
#[tokio::test]
async fn an_archived_workspace_cant_be_asked_its_account_so_asking_starts_no_worker() {
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

    let refused = hub.account_of(&workspace.id, chat.agent).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::Conflict, "{refused:?}");
    assert_eq!(asked.requests(everything).len(), asked_before, "no worker was asked");
    assert_eq!(hub.workers.lock().contains_key(&workspace.id), worker_before, "nor started");
}
