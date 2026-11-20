//! Conversation mode: which device has a chat's conversation, as starting, taking over and ending it leave the chat's
//! record and log, and how far that device has read the agent's reply out.

use std::sync::Arc;

use protocol::client::ErrorCode;
use protocol::event::EventBody;
use protocol::model::{ClientDevice, CutBy, Seq, SessionId, SpokenThrough};
use protocol::prompt::Prompt;

use crate::hub::Hub;
use crate::hub::stand_in_workers::{a_chat, hub_with_stand_in_workers};

fn device(id: &str, name: &str) -> ClientDevice {
    ClientDevice { id: id.into(), name: name.into() }
}

/// `through` bytes into the first block of the agent's message `message`.
fn read_out(message: &str, through: u32) -> SpokenThrough {
    SpokenThrough { message: message.into(), block: 0, through }
}

/// Every cut in the chat's log, oldest first.
fn cuts(hub: &Hub, chat: &SessionId) -> Vec<(SpokenThrough, CutBy)> {
    let events = hub.store.read_events(chat, Seq::FIRST, 1000, u64::MAX, false).unwrap();
    let cut = |body| {
        let EventBody::VoiceCut { through, by } = body else { return None };
        Some((through, by))
    };
    events.into_iter().filter_map(|e| cut(e.body)).collect()
}

/// How far the tail a new subscriber to the chat would be sent says the reply has been read out.
async fn speaking(hub: &Arc<Hub>, chat: &SessionId) -> Option<SpokenThrough> {
    hub.live_session(chat).await.unwrap().snapshot().tail.speaking
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

/// Only the device that has the conversation reads out, so only it says how far it has got, or that it was cut off. A
/// cut is recorded once for a message, however often it's sent, and the tail stops saying how far the reply has got.
#[tokio::test]
async fn only_the_device_with_the_conversation_says_how_far_it_has_read_and_a_cut_is_recorded_once() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    let (studio, air) = (device("d1", "Mac Studio"), device("d2", "Ada's MacBook Air"));
    hub.session_conversation_start(chat.id.clone(), studio.clone()).await.unwrap();

    let elsewhere = hub.session_speaking(chat.id.clone(), &air, Some(read_out("m1", 5))).await;
    let cut_elsewhere = hub.session_voice_cut(chat.id.clone(), &air, read_out("m1", 5), CutBy::Stopped).await;

    assert_eq!(elsewhere.map_err(|e| e.code).err(), Some(ErrorCode::Conflict));
    assert_eq!(cut_elsewhere.map_err(|e| e.code).err(), Some(ErrorCode::Conflict));
    assert_eq!((speaking(&hub, &chat.id).await, cuts(&hub, &chat.id)), (None, Vec::new()));

    hub.session_speaking(chat.id.clone(), &studio, Some(read_out("m1", 12))).await.unwrap();
    assert_eq!(speaking(&hub, &chat.id).await, Some(read_out("m1", 12)));

    for through in [12, 12, 30] {
        hub.session_voice_cut(chat.id.clone(), &studio, read_out("m1", through), CutBy::TalkedOver).await.unwrap();
    }

    assert_eq!(cuts(&hub, &chat.id), [(read_out("m1", 12), CutBy::TalkedOver)], "once for the message");
    assert_eq!(speaking(&hub, &chat.id).await, None, "it's no longer being read out");
    hub.session_speaking(chat.id.clone(), &studio, Some(read_out("m2", 4))).await.unwrap();
    hub.session_speaking(chat.id.clone(), &studio, None).await.unwrap();
    assert_eq!(speaking(&hub, &chat.id).await, None, "read to its end");
    assert_eq!(cuts(&hub, &chat.id).len(), 1, "which records nothing");
}

/// The device a conversation leaves stops reading out, so taking the conversation over, or turning the mode off, while
/// a reply is read out cuts it off where it had got to, as stopped.
#[tokio::test]
async fn taking_it_over_or_turning_it_off_while_a_reply_is_read_out_cuts_it_off_as_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    let (studio, air) = (device("d1", "Mac Studio"), device("d2", "Ada's MacBook Air"));
    hub.session_conversation_start(chat.id.clone(), studio.clone()).await.unwrap();
    hub.session_speaking(chat.id.clone(), &studio, Some(read_out("m1", 12))).await.unwrap();

    hub.session_conversation_start(chat.id.clone(), air.clone()).await.unwrap();

    assert_eq!(cuts(&hub, &chat.id), [(read_out("m1", 12), CutBy::Stopped)]);
    assert_eq!(speaking(&hub, &chat.id).await, None);
    let left = hub.session_speaking(chat.id.clone(), &studio, Some(read_out("m1", 20))).await;
    assert_eq!(left.map_err(|e| e.code).err(), Some(ErrorCode::Conflict), "the device it left has stopped");

    hub.session_speaking(chat.id.clone(), &air, Some(read_out("m2", 7))).await.unwrap();
    hub.session_conversation_end(chat.id.clone()).await.unwrap();

    assert_eq!(cuts(&hub, &chat.id), [(read_out("m1", 12), CutBy::Stopped), (read_out("m2", 7), CutBy::Stopped)]);
    assert_eq!(speaking(&hub, &chat.id).await, None);
}
