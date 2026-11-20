//! Conversation mode: which device has a chat's conversation, as starting, taking over and ending it leave the chat's
//! record and log.

use protocol::client::ErrorCode;
use protocol::event::EventBody;
use protocol::model::{ClientDevice, Seq};
use protocol::prompt::Prompt;

use crate::hub::stand_in_workers::{a_chat, hub_with_stand_in_workers};

fn device(id: &str, name: &str) -> ClientDevice {
    ClientDevice { id: id.into(), name: name.into() }
}

/// Turning it on records the rule once, on whichever device turned it on. Starting it from another device takes it
/// over, recording nothing, and from the same one changes nothing. Turning it on again after it's off records another.
#[tokio::test]
async fn turning_it_on_records_the_rule_once_and_another_device_takes_it_over_without_one() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    let (studio, air) = (device("d1", "Mac Studio"), device("d2", "Ada's MacBook Air"));
    let rules = || -> Vec<(Seq, ClientDevice)> {
        let events = hub.store.read_events(&chat.id, Seq::FIRST, 1000, u64::MAX, false).unwrap();
        events
            .into_iter()
            .filter_map(|e| {
                let EventBody::ConversationStarted { device } = e.body else { return None };
                Some((e.seq, device))
            })
            .collect()
    };

    let started = hub.session_conversation_start(chat.id.clone(), studio.clone()).await.unwrap();
    let again = hub.session_conversation_start(chat.id.clone(), studio.clone()).await.unwrap();
    let taken_over = hub.session_conversation_start(chat.id.clone(), air.clone()).await.unwrap();

    assert_eq!(started.conversation.as_deref(), Some(&studio));
    assert_eq!(again.rev, started.rev, "the same device again changes nothing");
    assert_eq!(taken_over.conversation.as_deref(), Some(&air));
    assert_eq!(hub.store.get_session(&chat.id).unwrap().session.conversation.as_deref(), Some(&air));
    let [(first, on)] = &rules()[..] else { panic!("one rule: {:?}", rules()) };
    assert_eq!(on, &studio);

    let ended = hub.session_conversation_end(chat.id.clone()).await.unwrap();
    let ended_again = hub.session_conversation_end(chat.id.clone()).await.unwrap();
    let back_on = hub.session_conversation_start(chat.id.clone(), air.clone()).await.unwrap();

    assert_eq!(ended.conversation, None);
    assert_eq!(ended_again.rev, ended.rev, "ending it again changes nothing");
    assert_eq!(back_on.conversation.as_deref(), Some(&air));
    let [(_, _), (second, on)] = &rules()[..] else { panic!("two rules: {:?}", rules()) };
    assert!(second > first);
    assert_eq!(on, &air);
}

#[tokio::test]
async fn a_closed_chat_cant_have_it_turned_on() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    // Something sent, so closing the chat keeps it.
    hub.session_prompt(chat.id.clone(), None, Prompt::new("hi")).await.unwrap();
    hub.session_close(chat.id.clone()).await.unwrap();

    let refused = hub.session_conversation_start(chat.id.clone(), device("d1", "Mac Studio")).await;

    assert_eq!(refused.map_err(|e| e.code).err(), Some(ErrorCode::Conflict));
    assert_eq!(hub.store.get_session(&chat.id).unwrap().session.conversation, None);
}
