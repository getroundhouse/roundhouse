//! How far the device that has the conversation has read a reply out, live, and what the hub settles of the replies it
//! won't say the end of: the one being read once its connection closes, and every one left once the mode ends or
//! moves.

use std::sync::Arc;
use std::time::Duration;

use protocol::client::ErrorCode;
use protocol::event::{AgentRunState, ContentBlock, EventBody, TurnOutcome};
use protocol::fixtures::turn_ended;
use protocol::model::{ClientDevice, Ending, MessageId, ReplyHeard, ReplyPlace, SessionId, SettledBy, SpokenThrough};
use protocol::prompt::Prompt;
use tokio::sync::{broadcast, watch};

use crate::hub::Hub;
use crate::hub::stand_in_workers::{Asked, TIMEOUT, a_chat, a_turn, hub_with_stand_in_workers, the_agent_says};
use crate::session::{Live, LiveItem, ReadingOn};

fn device(id: &str, name: &str) -> ClientDevice {
    ClientDevice { id: id.into(), name: name.into() }
}

fn studio() -> ClientDevice {
    device("d1", "Mac Studio")
}

fn place(block: u32, through: u32) -> ReplyPlace {
    ReplyPlace { block, through }
}

fn at(message: &str, block: u32, through: u32) -> SpokenThrough {
    SpokenThrough { message: message.into(), through: place(block, through) }
}

/// A connection that can be closed, as a device's is when it goes.
struct Connection {
    id: u64,
    closed: watch::Sender<bool>,
}

impl Connection {
    fn new(id: u64) -> Self {
        Self { id, closed: watch::Sender::new(false) }
    }

    fn on(&self) -> ReadingOn {
        ReadingOn { connection: self.id, closed: self.closed.subscribe() }
    }
}

/// Where the chat's tail says its reply has been read to.
async fn speaking(hub: &Arc<Hub>, chat: &SessionId) -> Option<SpokenThrough> {
    hub.live_session(chat).await.unwrap().snapshot().tail.speaking
}

/// How many times the chat's followers have been told to read its tail again since `following` was last asked.
fn told_to_read_again(following: &mut broadcast::Receiver<Live>) -> usize {
    std::iter::from_fn(|| following.try_recv().ok()).filter(|live| matches!(live.item, LiveItem::Rewritten)).count()
}

fn heard(hub: &Hub, chat: &SessionId, message: &str) -> Option<ReplyHeard> {
    hub.store.reply_heard(chat, &MessageId::from(message)).unwrap()
}

/// The agent's own reply `message`, saying something, or thinking only.
fn reply(message: &str, says: bool) -> EventBody {
    let block = if says {
        ContentBlock::Text { text: format!("{message} says"), images: Vec::new() }
    } else {
        ContentBlock::Thinking { text: "thinking".into() }
    };
    EventBody::Assistant {
        message_id: Some(message.into()),
        subagent: None,
        model: None,
        blocks: vec![block],
        first_index: None,
    }
}

/// The place moves only forward, and goes once the device says how the reply ended. A place in a reply whose ending is
/// kept is too late, since the device's requests can land out of order. Only the device that has the conversation says.
#[tokio::test]
async fn the_place_being_read_moves_only_forward_and_goes_with_its_replys_end() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    hub.session_conversation_start(chat.id.clone(), studio()).await.unwrap();
    let (connection, studio, air) = (Connection::new(1), studio(), device("d2", "Air"));
    let say = |through| hub.session_speaking(chat.id.clone(), Some(&studio), connection.on(), through);

    let mut following = hub.live_session(&chat.id).await.unwrap().subscribe();

    say(at("m1", 1, 10)).await.unwrap();
    assert_eq!(told_to_read_again(&mut following), 1);
    say(at("m1", 0, 40)).await.unwrap();
    let elsewhere = hub.session_speaking(chat.id.clone(), Some(&air), connection.on(), at("m1", 2, 0));
    assert_eq!(elsewhere.await.map_err(|e| e.code).err(), Some(ErrorCode::Conflict));
    assert_eq!(speaking(&hub, &chat.id).await, Some(at("m1", 1, 10)), "an earlier place changes nothing");
    assert_eq!(told_to_read_again(&mut following), 0);

    hub.session_reply_heard(chat.id.clone(), Some(&studio), "m1".into(), Ending::ToTheEnd).await.unwrap();
    assert_eq!(speaking(&hub, &chat.id).await, None);
    assert_eq!(told_to_read_again(&mut following), 1);
    say(at("m1", 1, 20)).await.unwrap();
    assert_eq!(speaking(&hub, &chat.id).await, None, "too late for a reply that has ended");
}

/// The reply being read on a connection that closes is settled as stopped where it had got to, and the device, which
/// knows, can go on reading it on another and put that right while it has the conversation, once. A connection that closes after the device has gone on
/// reading on another settles nothing, and the other settles its reply when it closes.
#[tokio::test]
async fn a_closed_connection_settles_its_reply_as_stopped_and_the_device_can_put_it_right() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    hub.session_conversation_start(chat.id.clone(), studio()).await.unwrap();
    let (first, second, third) = (Connection::new(1), Connection::new(2), Connection::new(3));
    let on_the_studio = studio();
    let on = |connection: &Connection, through| {
        hub.session_speaking(chat.id.clone(), Some(&on_the_studio), connection.on(), through)
    };
    on(&first, at("m1", 0, 12)).await.unwrap();

    first.closed.send_replace(true);
    waiting::task::until(TIMEOUT, "the reply settled", || heard(&hub, &chat.id, "m1").is_some()).await;

    let settled = heard(&hub, &chat.id, "m1").unwrap();
    assert_eq!((settled.ending, settled.settled_by), (Ending::Stopped { through: place(0, 12) }, SettledBy::Hub));
    assert_eq!(speaking(&hub, &chat.id).await, None);
    on(&second, at("m1", 0, 15)).await.unwrap();
    assert_eq!(speaking(&hub, &chat.id).await, Some(at("m1", 0, 15)), "a device that connects again goes on reading");
    let said = |ending| hub.session_reply_heard(chat.id.clone(), Some(&on_the_studio), "m1".into(), ending);
    said(Ending::ToTheEnd).await.unwrap();
    said(Ending::TalkedOver { through: place(0, 3) }).await.unwrap();
    let put_right = heard(&hub, &chat.id, "m1").unwrap();
    assert_eq!((put_right.ending, put_right.settled_by), (Ending::ToTheEnd, SettledBy::Device));

    on(&second, at("m2", 0, 5)).await.unwrap();
    on(&third, at("m2", 0, 9)).await.unwrap();
    second.closed.send_replace(true);
    waiting::task::for_what_mustnt_happen(Duration::from_millis(200)).await;
    assert_eq!(heard(&hub, &chat.id, "m2"), None, "still read on the third");
    assert_eq!(speaking(&hub, &chat.id).await, Some(at("m2", 0, 9)));

    third.closed.send_replace(true);
    waiting::task::until(TIMEOUT, "the reply settled", || heard(&hub, &chat.id, "m2").is_some()).await;
    assert_eq!(heard(&hub, &chat.id, "m2").unwrap().ending, Ending::Stopped { through: place(0, 9) });
}

/// How the conversation leaves the device that had it.
#[derive(Clone, Copy, Debug)]
enum Leaving {
    TurnedOff,
    Typed,
    TakenOver,
}

/// A chat whose agent answered something said out loud with three replies, the first being read out, after a reply it
/// wrote before the mode was on. Returns the chat, and the connection it's read on, which is closed once dropped.
async fn three_replies_in_the_mode(hub: &Arc<Hub>, asked: &Asked) -> (SessionId, Connection) {
    let (_, chat) = a_chat(hub).await;
    a_turn(hub, asked, &chat.id, Prompt::new("typed"), None).await;
    the_agent_says(hub, asked, &chat.id, [reply("m0", true)]).await;
    hub.session_conversation_start(chat.id.clone(), studio()).await.unwrap();
    let said = Prompt::new("what did you find?").said_out_loud();
    let (said, _) = hub.session_prompt(chat.id.clone(), None, said, Some(studio())).await.unwrap();
    waiting::task::until(TIMEOUT, "the agent was given it", || asked.prompts_given(&chat.id).contains(&said)).await;
    let answered = [
        EventBody::PromptTaken { prompt: said },
        reply("m1", true),
        reply("m2", true),
        reply("m3", false),
        turn_ended(TurnOutcome::Success),
        EventBody::AgentState { state: AgentRunState::Idle },
    ];
    the_agent_says(hub, asked, &chat.id, answered).await;
    let reading = Connection::new(1);
    hub.session_speaking(chat.id.clone(), Some(&studio()), reading.on(), at("m1", 0, 3)).await.unwrap();
    (chat.id, reading)
}

/// However the conversation leaves the device, by turning the mode off, by typing, or by another device taking it
/// over, the reply being read is settled as stopped where it had got to, and every other the agent wrote in the mode
/// with something to say, as not read out. A reply from before the mode, and one with nothing to say, are left.
#[tokio::test]
async fn leaving_the_conversation_settles_the_reply_being_read_and_every_other_not_read_out() {
    for leaving in [Leaving::TurnedOff, Leaving::Typed, Leaving::TakenOver] {
        let dir = tempfile::tempdir().unwrap();
        let (hub, asked) = hub_with_stand_in_workers(dir.path());
        let (chat, _reading) = three_replies_in_the_mode(&hub, &asked).await;

        let left = match leaving {
            Leaving::TurnedOff => hub.session_conversation_end(chat.clone()).await.map(drop),
            Leaving::Typed => hub.session_prompt(chat.clone(), None, Prompt::new("never mind"), None).await.map(drop),
            Leaving::TakenOver => hub.session_conversation_start(chat.clone(), device("d2", "Air")).await.map(drop),
        };
        left.unwrap();

        let ending = |message| heard(&hub, &chat, message).map(|heard| (heard.ending, heard.settled_by));
        let stopped = Ending::Stopped { through: place(0, 3) };
        assert_eq!(ending("m1"), Some((stopped, SettledBy::Hub)), "{leaving:?}");
        assert_eq!(ending("m2"), Some((Ending::NotReadOut, SettledBy::Hub)), "{leaving:?}");
        assert_eq!((ending("m0"), ending("m3")), (None, None), "{leaving:?}");
        assert_eq!(speaking(&hub, &chat).await, None, "{leaving:?}");
    }
}
