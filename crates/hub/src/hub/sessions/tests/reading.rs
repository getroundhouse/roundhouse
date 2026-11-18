//! What a read of a chat's events is sent, on a hub whose workspaces have stand-ins for their workers. How much a read
//! is sent is `protocol`'s `ReadLimit`, as its tests say, and these show the hub reads by it.

use protocol::model::{Place, Seq};
use protocol::prompt::Prompt;

use crate::hub::stand_in_workers::{a_chat, a_turn, hub_with_stand_in_workers};

#[tokio::test]
async fn a_read_asking_for_no_events_is_sent_one_by_seq_and_by_place() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    a_turn(&hub, &asked, &chat.id, Prompt::new("look around"), None).await;

    let by_seq = hub.session_read(chat.id.clone(), Seq::new(1), 0, None, false).await.unwrap();
    let by_place = hub.session_read_places(chat.id.clone(), None, Place::new(1), 0, None, false).await.unwrap();

    assert_eq!(by_seq.events.len(), 1);
    assert_eq!(by_place.events.len(), 1);
}
