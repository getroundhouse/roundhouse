//! Conversation mode: which device has a chat's conversation, as starting, taking over and ending it leave the chat's
//! record and log.

use protocol::client::{ErrorCode, HubNotice};
use protocol::event::{AgentRunState, EventBody, TurnOutcome};
use protocol::fixtures::turn_ended;
use protocol::model::{ClientDevice, Ending, Interaction, InteractionMode, ReplyPlace, Seq};
use protocol::prompt::Prompt;

use crate::hub::stand_in_workers::{TIMEOUT, a_chat, a_turn, hub_with_stand_in_workers, the_agent_says};

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

    assert_eq!(started.interaction.device(), Some(&studio));
    assert_eq!(again.rev, started.rev, "the same device again changes nothing");
    assert_eq!(taken_over.interaction.device(), Some(&air));
    assert_eq!(hub.store.get_session(&chat.id).unwrap().session.interaction.device(), Some(&air));
    let [(first, on)] = &rules()[..] else { panic!("one rule: {:?}", rules()) };
    assert_eq!(on, &studio);

    let ended = hub.session_conversation_end(chat.id.clone()).await.unwrap();
    let ended_again = hub.session_conversation_end(chat.id.clone()).await.unwrap();
    let back_on = hub.session_conversation_start(chat.id.clone(), air.clone()).await.unwrap();

    assert_eq!(ended.interaction, Interaction::Typing);
    assert_eq!(ended_again.rev, ended.rev, "ending it again changes nothing");
    assert_eq!(back_on.interaction.device(), Some(&air));
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
    hub.session_prompt(chat.id.clone(), None, Prompt::new("hi"), None).await.unwrap();
    hub.session_close(chat.id.clone()).await.unwrap();

    let refused = hub.session_conversation_start(chat.id.clone(), device("d1", "Mac Studio")).await;

    assert_eq!(refused.map_err(|e| e.code).err(), Some(ErrorCode::Conflict));
    assert_eq!(hub.store.get_session(&chat.id).unwrap().session.interaction, Interaction::Typing);
}

/// The same device by its ID, under another name, as when the computer's been renamed, has the conversation still, so
/// it changes nothing.
#[tokio::test]
async fn a_device_renamed_still_has_the_conversation() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    let started = hub.session_conversation_start(chat.id.clone(), device("d1", "Mac Studio")).await.unwrap();

    let renamed = hub.session_conversation_start(chat.id.clone(), device("d1", "Studio")).await.unwrap();

    assert_eq!(renamed.rev, started.rev);
    let said = Prompt::new("still mine").said_out_loud();
    hub.session_prompt(chat.id.clone(), None, said, Some(device("d1", "Studio"))).await.unwrap();
}

/// Every event says how the person was talking as it was recorded. The rule that turns the mode on is the first of the
/// conversation, what the agent does while it's on is in it, and a prompt typed while it's on turns it off in the same
/// commit, so it's typed, as the record is once it's recorded.
#[tokio::test]
async fn every_event_is_recorded_under_the_mode_and_a_typed_prompt_turns_it_off() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    let studio = device("d1", "Mac Studio");
    a_turn(&hub, &asked, &chat.id, Prompt::new("typed first"), None).await;
    hub.session_conversation_start(chat.id.clone(), studio.clone()).await.unwrap();
    let said = Prompt::new("said out loud").said_out_loud();
    let (said, _) = hub.session_prompt(chat.id.clone(), None, said, Some(studio.clone())).await.unwrap();
    waiting::task::until(TIMEOUT, "the agent was given what was said", || {
        asked.prompts_given(&chat.id).contains(&said)
    })
    .await;
    let answered = [
        EventBody::PromptTaken { prompt: said },
        turn_ended(TurnOutcome::Success),
        EventBody::AgentState { state: AgentRunState::Idle },
    ];
    the_agent_says(&hub, &asked, &chat.id, answered).await;
    // From the device that has it, as typing there would be, and from any other alike.
    let (typed, _) =
        hub.session_prompt(chat.id.clone(), None, Prompt::new("typed"), Some(studio.clone())).await.unwrap();
    let after = hub.session_prompt(chat.id.clone(), None, Prompt::new("too late").said_out_loud(), Some(studio)).await;

    let events = hub.store.read_events(&chat.id, Seq::FIRST, 1000, u64::MAX, false).unwrap();
    let rule = events.iter().find(|e| matches!(e.body, EventBody::ConversationStarted { .. })).unwrap();
    let modes: Vec<(&str, InteractionMode)> =
        events.iter().filter(|e| e.seq >= rule.seq && e.seq <= typed).map(|e| (e.body.kind(), e.interaction)).collect();
    let conversation = InteractionMode::Conversation;
    let expected = [
        ("conversation_started", conversation),
        ("user_prompt", conversation),
        ("prompt_taken", conversation),
        ("turn_ended", conversation),
        ("agent_state", conversation),
        ("user_prompt", InteractionMode::Typing),
    ];
    assert_eq!(modes, expected);
    assert!(events.iter().filter(|e| e.seq < rule.seq).all(|e| e.interaction == InteractionMode::Typing));
    assert_eq!(hub.store.get_session(&chat.id).unwrap().session.interaction, Interaction::Typing);
    assert_eq!(after.map_err(|e| e.code).err(), Some(ErrorCode::Conflict), "nothing can be said once it's off");
}

/// Only the device that has the conversation can say anything out loud, sent or queued, so a spoken prompt is always
/// one the conversation heard. Nothing's recorded or queued for one refused.
#[tokio::test]
async fn only_the_device_with_the_conversation_can_say_a_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    let (studio, air) = (device("d1", "Mac Studio"), device("d2", "Ada's MacBook Air"));
    let said = || Prompt::new("hello").said_out_loud();

    let while_off = hub.session_prompt(chat.id.clone(), None, said(), Some(studio.clone())).await;
    hub.session_conversation_start(chat.id.clone(), studio.clone()).await.unwrap();
    let elsewhere = hub.session_prompt(chat.id.clone(), None, said(), Some(air.clone())).await;
    let unsaid = hub.session_prompt(chat.id.clone(), None, said(), None).await;
    let queued_elsewhere = hub.session_queue(chat.id.clone(), "q".into(), said(), Some(&air)).await;

    for refused in [while_off.map(drop), elsewhere.map(drop), unsaid.map(drop), queued_elsewhere.map(drop)] {
        assert_eq!(refused.map_err(|e| e.code).err(), Some(ErrorCode::Conflict));
    }
    let events = hub.store.read_events(&chat.id, Seq::FIRST, 1000, u64::MAX, false).unwrap();
    assert!(!events.iter().any(|e| matches!(e.body, EventBody::UserPrompt { .. })), "{events:?}");
    let record = hub.store.get_session(&chat.id).unwrap().session;
    assert!(record.queued.is_empty());
    assert_eq!(record.interaction.device(), Some(&studio), "a refusal leaves the mode on");
}

/// The device that has the conversation says how each reply it read out ended, which every device following the feed
/// is told and can list. The first it says of a reply is kept, and another device, or one in a chat whose mode is off,
/// is refused, with nothing kept.
#[tokio::test]
async fn the_device_with_the_conversation_says_how_each_reply_ended_once() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    let (studio, air) = (device("d1", "Mac Studio"), device("d2", "Ada's MacBook Air"));
    let cut = Ending::TalkedOver { through: ReplyPlace { block: 0, through: 12 } };
    let while_off = hub.session_reply_heard(chat.id.clone(), Some(&studio), "m0".into(), Ending::ToTheEnd).await;
    hub.session_conversation_start(chat.id.clone(), studio.clone()).await.unwrap();
    let mut feed = hub.subscribe_feed();

    hub.session_reply_heard(chat.id.clone(), Some(&studio), "m1".into(), cut).await.unwrap();
    hub.session_reply_heard(chat.id.clone(), Some(&studio), "m1".into(), Ending::ToTheEnd).await.unwrap();
    let elsewhere = hub.session_reply_heard(chat.id.clone(), Some(&air), "m2".into(), Ending::ToTheEnd).await;
    let unnamed = hub.session_reply_heard(chat.id.clone(), None, "m2".into(), Ending::ToTheEnd).await;

    for refused in [while_off, elsewhere, unnamed] {
        assert_eq!(refused.map_err(|e| e.code).err(), Some(ErrorCode::Conflict));
    }
    let told: Vec<_> = std::iter::from_fn(|| feed.try_recv().ok().map(std::sync::Arc::unwrap_or_clone))
        .filter_map(|notice| if let HubNotice::ReplyHeard(heard) = notice { Some(heard) } else { None })
        .collect();
    let listed = hub.session_records(chat.id.clone()).await.unwrap();
    let [heard] = &listed.replies_heard[..] else { panic!("one reply: {listed:?}") };
    assert_eq!((&heard.message, heard.ending), (&"m1".into(), cut));
    assert_eq!(told, listed.replies_heard, "told once, of the first");
}
