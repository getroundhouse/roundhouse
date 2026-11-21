//! The thread that reads out, against a hub stood in for: what came before its tail isn't read, what comes after is,
//! and the hub is told of each reply in order.

use std::time::Duration;

use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, HubFrame, ReplyOutcome, conversation};
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::fixtures::{event, session, turn_ended};
use protocol::model::{Ending, InteractionMode, ReplyPlace, Seq, SpokenThrough};
use protocol::tail::{BlockKind, Tail, TailBlock, TailMessage};

use crate::conversation::{FromVoice, ReadingOut, ToVoice, Voice, VoiceText};
use crate::stand_in_hub::{self, Does};
use protocol::event::TurnOutcome;

const TIMEOUT: Duration = Duration::from_secs(10);

/// A voice that tells the test what it's given, and says what the test has it say.
struct FakeVoice {
    given: flume::Sender<ToVoice>,
    says: flume::Receiver<FromVoice>,
}

impl Voice for FakeVoice {
    type Said = FromVoice;

    fn act(&mut self, what: ToVoice) {
        let _ = self.given.send(what);
    }

    fn said(&self) -> flume::Receiver<FromVoice> {
        self.says.clone()
    }

    fn read(&mut self, said: FromVoice) -> Option<FromVoice> {
        Some(said)
    }
}

fn said(seq: u64, message: &str, words: &str) -> SessionEvent {
    said_in_block(seq, message, None, words)
}

/// The agent's message `message` recording `words` as its block `first_index`, in the mode.
fn said_in_block(seq: u64, message: &str, first_index: Option<u32>, words: &str) -> SessionEvent {
    let blocks = vec![ContentBlock::Text { text: words.into(), images: Vec::new() }];
    let body =
        EventBody::Assistant { message_id: Some(message.into()), subagent: None, model: None, blocks, first_index };
    SessionEvent { interaction: InteractionMode::Conversation, ..event(Seq::new(seq), body) }
}

fn streaming(message: &str, words: &str) -> Tail {
    let block = TailBlock { index: 0, kind: BlockKind::Text, tool_name: None, text: words.into() };
    let message = TailMessage { run: None, message_id: message.into(), subagent: None, blocks: vec![block] };
    Tail { messages: vec![message], ..Tail::default() }
}

/// What the hub is told of how far a reply has got and how it ended, as the stand-in says it was asked.
#[derive(Debug, PartialEq, Eq)]
enum Told {
    Speaking(SpokenThrough),
    Heard(String, Ending),
    /// Asked to turn the mode off.
    End,
}

/// A hub whose chat had `m0` begun before the subscription, `m1` streaming when its tail came, and after it the rest
/// of `m0`, `m1` recorded and `m2` said, then the turn's end.
fn hub(socket: &std::path::Path) -> (crate::Client, flume::Receiver<Told>) {
    let (tell, told) = flume::unbounded();
    let client = stand_in_hub::connect(socket, move |request| {
        let frame = |subscription_id, event| HubFrame::Event { subscription_id, event };
        let ok = || Box::new(ReplyOutcome::Response(ClientResponse::Ok));
        if let ClientRequest::SessionSubscribe(subscribe) = request {
            let id = subscribe.subscription_id;
            let ended = event(Seq::new(5), turn_ended(TurnOutcome::Success));
            return Does::AnswersThen(
                ok(),
                vec![
                    frame(id, said_in_block(1, "m0", Some(0), "Before.")),
                    HubFrame::Tail { subscription_id: id, tail: streaming("m1", "Half") },
                    frame(id, said_in_block(2, "m0", Some(1), "More of it.")),
                    frame(id, said(3, "m1", "Half of it.")),
                    frame(id, said(4, "m2", "Yes.")),
                    frame(id, SessionEvent { interaction: InteractionMode::Conversation, ..ended }),
                ],
            );
        }
        let heard = if let ClientRequest::SessionSpeaking(conversation::SessionSpeaking { through, .. }) = request {
            Told::Speaking(through)
        } else if let ClientRequest::SessionReplyHeard(conversation::SessionReplyHeard { message, ending, .. }) =
            request
        {
            Told::Heard(message.to_string(), ending)
        } else if let ClientRequest::SessionConversationEnd(_) = request {
            // Refused, as the test needs only to know it was asked, after what came before it.
            let _ = tell.send(Told::End);
            return Does::Answers(Box::new(ReplyOutcome::Error(ApiError::new(ErrorCode::Unsupported, "a stand-in"))));
        } else {
            return Does::Answers(Box::new(ReplyOutcome::Error(ApiError::new(ErrorCode::Unsupported, "a stand-in"))));
        };
        let _ = tell.send(heard);
        Does::Answers(ok())
    });
    (client, told)
}

/// Reading out the hub's chat, with a voice the test is given what's given to, and has say what it says.
fn reading_out(
    client: &crate::Client,
) -> (ReadingOut, flume::Receiver<ToVoice>, flume::Sender<FromVoice>, flume::Receiver<()>) {
    let (given, voice_was_given) = flume::unbounded();
    let (say, says) = flume::unbounded();
    let (tell, changed) = flume::bounded(1);
    let chat = protocol::model::Session { last_seq: Seq::new(1), ..session("s") };
    let reading = ReadingOut::start(client, &chat, FakeVoice { given, says }, tell);
    (reading, voice_was_given, say, changed)
}

/// What was recorded before the tail came, with what comes after of that reply, and what was under way in the tail,
/// isn't read, and nothing is said of it. What comes after is, and the hub is told how far it got and how each ended,
/// in order.
#[test]
fn only_what_comes_after_the_tail_is_read_and_the_hub_is_told_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let (client, told) = hub(&dir.path().join("hub.sock"));
    let (reading, voice_was_given, say, _changed) = reading_out(&client);

    let text = VoiceText(1);
    assert_eq!(voice_was_given.recv_timeout(TIMEOUT), Ok(ToVoice::Say { text, at: 0, more: "Yes.".into() }));
    assert_eq!(voice_was_given.recv_timeout(TIMEOUT), Ok(ToVoice::End(text)));
    assert_eq!(told.recv_timeout(TIMEOUT), Ok(Told::Heard("m1".into(), Ending::NotReadOut)));

    say.send(FromVoice::Heard { text, through: 4 }).unwrap();
    say.send(FromVoice::Finished(text)).unwrap();
    let through = SpokenThrough { message: "m2".into(), through: ReplyPlace { block: 0, through: 4 } };
    assert_eq!(told.recv_timeout(TIMEOUT), Ok(Told::Speaking(through)));
    assert_eq!(told.recv_timeout(TIMEOUT), Ok(Told::Heard("m2".into(), Ending::ToTheEnd)));

    reading.talk();
    drop(reading);
    assert_eq!(voice_was_given.recv_timeout(TIMEOUT), Ok(ToVoice::Stop), "dropping it stops the voice");
    assert!(told.recv_timeout(Duration::from_millis(100)).is_err(), "nothing of m0, said before: {told:?}");
}

/// Leaving stops the voice, tells the hub how the reply being read ended, where the voice says it stopped, and only
/// then turns the mode off, though the reading out has gone.
#[test]
fn leaving_tells_the_hub_how_the_reply_ended_before_the_mode_ends() {
    let dir = tempfile::tempdir().unwrap();
    let (client, told) = hub(&dir.path().join("hub.sock"));
    let (reading, voice_was_given, say, changed) = reading_out(&client);
    let text = VoiceText(1);
    voice_was_given.recv_timeout(TIMEOUT).unwrap();
    voice_was_given.recv_timeout(TIMEOUT).unwrap();
    told.recv_timeout(TIMEOUT).unwrap();
    say.send(FromVoice::Heard { text, through: 2 }).unwrap();
    told.recv_timeout(TIMEOUT).unwrap();
    changed.recv_timeout(TIMEOUT).unwrap();
    assert!(reading.reading_a_reply());

    reading.leave();
    assert_eq!(voice_was_given.recv_timeout(TIMEOUT), Ok(ToVoice::Stop));
    say.send(FromVoice::Stopped { cut: Some((text, 2)), never_started: Vec::new() }).unwrap();
    let stopped = Ending::Stopped { through: ReplyPlace { block: 0, through: 2 } };
    assert_eq!(told.recv_timeout(TIMEOUT), Ok(Told::Heard("m2".into(), stopped)));
    assert_eq!(told.recv_timeout(TIMEOUT), Ok(Told::End));
}

/// How a reply ended, told on a connection that goes as it's told, is told again on the next, so a device that read
/// to the end isn't shown as stopped where the hub last heard of it.
#[test]
fn how_a_reply_ended_is_told_again_on_a_new_connection() {
    let dir = tempfile::tempdir().unwrap();
    let first = stand_in_hub::connect(&dir.path().join("first.sock"), |request| {
        if let ClientRequest::SessionSubscribe(subscribe) = request {
            let id = subscribe.subscription_id;
            let ok = Box::new(ReplyOutcome::Response(ClientResponse::Ok));
            let ended = event(Seq::new(3), turn_ended(TurnOutcome::Success));
            let ended = SessionEvent { interaction: InteractionMode::Conversation, ..ended };
            return Does::AnswersThen(
                ok,
                vec![
                    HubFrame::Tail { subscription_id: id, tail: Tail::default() },
                    HubFrame::Event { subscription_id: id, event: said(2, "m1", "Yes.") },
                    HubFrame::Event { subscription_id: id, event: ended },
                ],
            );
        }
        if matches!(request, ClientRequest::SessionReplyHeard(_)) { Does::HangsUp } else { Does::Nothing }
    });
    let (reading, voice_was_given, say, _changed) = reading_out(&first);
    let text = VoiceText(1);
    voice_was_given.recv_timeout(TIMEOUT).unwrap();
    voice_was_given.recv_timeout(TIMEOUT).unwrap();
    say.send(FromVoice::Finished(text)).unwrap();
    waiting::thread::until(TIMEOUT, "the first connection to go", || first.is_disconnected());

    let (tell, told) = flume::unbounded();
    let second = stand_in_hub::connect(&dir.path().join("second.sock"), move |request| {
        if let ClientRequest::SessionReplyHeard(conversation::SessionReplyHeard { message, ending, .. }) = request {
            let _ = tell.send(Told::Heard(message.to_string(), ending));
        }
        Does::Answers(Box::new(ReplyOutcome::Response(ClientResponse::Ok)))
    });
    reading.reconnect(&second);

    assert_eq!(told.recv_timeout(TIMEOUT), Ok(Told::Heard("m1".into(), Ending::ToTheEnd)));
}
