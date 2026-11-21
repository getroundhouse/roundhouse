use protocol::event::{ContentBlock, EventBody};
use protocol::model::{ClientDevice, Ending, Interaction, MessageId, ReplyPlace, SettledBy};

use crate::store::SessionChange;
use crate::store::tests::store_with_session;

/// The first thing said of a reply is kept, so a device sending it again, or another of the same reply, changes
/// nothing, and each reply is listed with the newest revision of them all.
#[test]
fn the_first_thing_said_of_a_reply_is_kept_and_listed_by_its_chat() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let cut = Ending::TalkedOver { through: ReplyPlace { block: 1, through: 40 } };

    let first = store.keep_reply_heard(&"s".into(), &"m1".into(), cut, SettledBy::Device).unwrap().expect("kept");
    let again = store.keep_reply_heard(&"s".into(), &"m1".into(), Ending::ToTheEnd, SettledBy::Device).unwrap();
    let second = store
        .keep_reply_heard(&"s".into(), &"m2".into(), Ending::NotReadOut, SettledBy::Device)
        .unwrap()
        .expect("kept");
    let (listed, rev) = store.list_replies_heard(&"s".into()).unwrap();

    assert_eq!(again, None);
    assert_eq!(first.ending, cut);
    assert!(second.rev.supersedes(first.rev));
    assert_eq!(listed, [first, second.clone()]);
    assert_eq!(rev, second.rev);
}

/// The agent's own reply `message`, with a text block if it `says` something, from `subagent` if one wrote it.
fn reply(message: &str, says: bool, subagent: Option<&str>) -> EventBody {
    let block = if says {
        ContentBlock::Text { text: "said".into(), images: Vec::new() }
    } else {
        ContentBlock::Thinking { text: "thought".into() }
    };
    EventBody::Assistant {
        message_id: Some(message.into()),
        subagent: subagent.map(Into::into),
        model: None,
        blocks: vec![block],
        first_index: None,
    }
}

/// Of the replies the agent wrote in conversation mode, those whose ending isn't kept, and that say something, are
/// left unheard, in the order they were written. A reply recorded in parts says something if any part does. A
/// subagent's reply, and one from before the mode, are never read out.
#[test]
fn the_replies_left_unheard_are_the_agents_own_in_the_mode_that_say_something() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let append = |body: EventBody, change| store.append(&"s".into(), None, &body, None, None, change).unwrap();
    let conversation =
        Interaction::Conversation { device: Box::new(ClientDevice { id: "d1".into(), name: "Mac".into() }) };

    append(reply("m0", true, None), None);
    append(EventBody::Unknown, Some(SessionChange { interaction: Some(conversation), ..SessionChange::default() }));
    for body in [
        reply("m3", true, None),
        reply("m1", false, None),
        reply("m1", true, None),
        reply("m2", false, None),
        reply("m4", true, Some("toolu_1")),
        reply("m5", true, None),
    ] {
        append(body, None);
    }
    store.keep_reply_heard(&"s".into(), &"m5".into(), Ending::ToTheEnd, SettledBy::Device).unwrap();

    assert_eq!(store.replies_unheard(&"s".into()).unwrap(), [MessageId::from("m3"), MessageId::from("m1")]);
}

/// What the hub settled of a reply gives way to what the device says of it, once.
#[test]
fn what_the_device_says_takes_the_place_of_what_the_hub_settled() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    let keep = |ending, by| store.keep_reply_heard(&"s".into(), &"m1".into(), ending, by).unwrap();
    let stopped = Ending::Stopped { through: ReplyPlace { block: 0, through: 7 } };

    let settled = keep(stopped, SettledBy::Hub).expect("kept");
    assert_eq!(keep(Ending::NotReadOut, SettledBy::Hub), None, "the hub settles a reply once");
    let put_right = keep(Ending::ToTheEnd, SettledBy::Device).expect("put right");
    assert_eq!(keep(stopped, SettledBy::Device), None);

    assert_eq!((settled.ending, settled.settled_by), (stopped, SettledBy::Hub));
    assert_eq!((put_right.ending, put_right.settled_by), (Ending::ToTheEnd, SettledBy::Device));
    assert!(put_right.rev.supersedes(settled.rev));
}
