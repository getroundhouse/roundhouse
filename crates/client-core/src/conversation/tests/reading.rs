//! Which replies the device reads out, what it gives the voice, and what it tells the hub of each.

use protocol::capability::calls::Call;
use protocol::capability::output::{CoverLine, CoverWait};
use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::fixtures::{event, turn_ended, typed_call, typed_command};
use protocol::model::{Ending, InteractionMode, ReplyPlace, RunId, Seq, SpokenThrough};
use protocol::tail::{BlockKind, Covering, Tail, TailBlock, TailMessage};

use crate::conversation::{Do, FromVoice, Reading, ToHub, ToVoice, VoiceText};
use protocol::event::TurnOutcome;

fn text(text: &str) -> ContentBlock {
    ContentBlock::Text { text: text.into(), images: Vec::new() }
}

fn call() -> ContentBlock {
    typed_call("c1", Call::Command(typed_command("ls")))
}

/// The agent's message `message` recording `blocks`, the first its block `first`, in conversation mode.
fn said(seq: u64, message: &str, first: Option<u32>, blocks: Vec<ContentBlock>) -> SessionEvent {
    let body = EventBody::Assistant {
        message_id: Some(message.into()),
        subagent: None,
        model: None,
        blocks,
        first_index: first,
    };
    SessionEvent { interaction: InteractionMode::Conversation, ..event(Seq::new(seq), body) }
}

fn in_mode(seq: u64, body: EventBody) -> SessionEvent {
    SessionEvent { interaction: InteractionMode::Conversation, ..event(Seq::new(seq), body) }
}

/// A tail with the agent's message `message` streaming each of `blocks`, by its index and what it holds so far.
fn streaming(message: &str, blocks: &[(u32, &str)]) -> Tail {
    let blocks = blocks
        .iter()
        .map(|(index, text)| TailBlock { index: *index, kind: BlockKind::Text, tool_name: None, text: (*text).into() })
        .collect();
    let message = TailMessage { run: None, message_id: message.into(), subagent: None, blocks };
    Tail { messages: vec![message], ..Tail::default() }
}

fn covered(wait: u32, number: u32, line: &str) -> Tail {
    let line = CoverLine { wait: CoverWait(wait), number, text: line.into() };
    Tail { covering: Some(Box::new(Covering { run: Some(RunId::new(1)), line })), ..Tail::default() }
}

fn say(text: u64, at: usize, more: &str) -> Do {
    Do::Voice(ToVoice::Say { text: VoiceText(text), at, more: more.into() })
}

fn end(text: u64) -> Do {
    Do::Voice(ToVoice::End(VoiceText(text)))
}

fn heard(message: &str, ending: Ending) -> Do {
    Do::Hub(ToHub::Heard { message: message.into(), ending })
}

fn speaking(message: &str, block: u32, through: u32) -> Do {
    Do::Hub(ToHub::Speaking(SpokenThrough { message: message.into(), through: ReplyPlace { block, through } }))
}

fn place(block: u32, through: u32) -> ReplyPlace {
    ReplyPlace { block, through }
}

fn voice_heard(text: u64, through: usize) -> FromVoice {
    FromVoice::Heard { text: VoiceText(text), through }
}

fn finished(text: u64) -> FromVoice {
    FromVoice::Finished(VoiceText(text))
}

fn ended() -> SessionEvent {
    in_mode(90, turn_ended(TurnOutcome::Success))
}

/// A reply is given to the voice as it streams, ended once it's recorded, and said to be heard to its end once the
/// voice has finished it and the agent has gone on past it, with how far it had got said as the voice went.
#[test]
fn a_reply_is_read_as_it_streams_and_heard_to_its_end_once_the_agent_goes_on() {
    let mut reading = Reading::from(&Tail::default());

    assert_eq!(reading.streaming(&streaming("m1", &[(0, "Sure, I")])), [say(1, 0, "Sure, I")]);
    assert_eq!(reading.streaming(&streaming("m1", &[(0, "Sure, I looked.")])), [say(1, 7, " looked.")]);
    assert_eq!(reading.recorded(&said(2, "m1", Some(0), vec![text("Sure, I looked.")])), [end(1)]);
    assert_eq!(reading.voice(voice_heard(1, 5)), [speaking("m1", 0, 5)]);
    assert_eq!(reading.voice(finished(1)), [], "the agent may write more of it");
    assert!(!reading.reading());

    assert_eq!(reading.recorded(&ended()), [heard("m1", Ending::ToTheEnd)]);
}

/// Each text block is a text of its own, by its index in the message, and the message is heard to its end only once
/// all of them are. One that never streamed is numbered by its place in its event, and given whole.
#[test]
fn each_block_is_read_by_its_index_and_one_that_never_streamed_whole() {
    let mut reading = Reading::from(&Tail::default());
    reading.streaming(&streaming("m1", &[(0, "One.")]));
    reading.recorded(&said(2, "m1", Some(0), vec![text("One.")]));

    assert_eq!(reading.recorded(&said(3, "m1", Some(2), vec![text("Two.")])), [say(2, 0, "Two."), end(2)]);
    assert_eq!(reading.voice(voice_heard(2, 4)), [speaking("m1", 2, 4)], "block 2, as the deltas numbered it");
    assert_eq!(reading.recorded(&said(4, "m2", None, vec![call(), text("Three.")])), [say(3, 0, "Three."), end(3)]);
    assert_eq!(reading.voice(voice_heard(3, 6)), [speaking("m2", 1, 6)], "by its place in its event");

    reading.voice(finished(2));
    assert_eq!(reading.voice(finished(1)), [heard("m1", Ending::ToTheEnd)]);
}

/// A message's calls come after its text, so a recorded call ends the reply, which is heard to its end as soon as
/// the voice finishes it, rather than once the call is answered.
#[test]
fn a_recorded_call_ends_the_reply() {
    let mut reading = Reading::from(&Tail::default());
    reading.streaming(&streaming("m1", &[(0, "Looking.")]));
    reading.recorded(&said(2, "m1", Some(0), vec![text("Looking.")]));

    assert_eq!(reading.recorded(&said(3, "m1", Some(1), vec![call()])), []);
    assert_eq!(reading.voice(finished(1)), [heard("m1", Ending::ToTheEnd)]);
}

/// A recorded block that doesn't follow on from what streamed gives the voice nothing more: the block ends where it
/// is, rather than the voice saying what it has said again.
#[test]
fn a_recorded_block_that_differs_from_what_streamed_gives_nothing_more() {
    let mut reading = Reading::from(&Tail::default());
    reading.streaming(&streaming("m1", &[(0, "Sure, I looked.")]));

    assert_eq!(reading.recorded(&said(2, "m1", Some(0), vec![text("Sure.")])), [end(1)]);
}

/// Only the agent's own replies in the mode are read: not a subagent's, and not one recorded once the mode was off.
#[test]
fn only_the_agents_own_replies_in_the_mode_are_read() {
    let mut reading = Reading::from(&Tail::default());
    let typed = SessionEvent { interaction: InteractionMode::Typing, ..said(2, "m1", None, vec![text("Typed.")]) };
    let subagent = SessionEvent { subagent: Some("toolu-1".into()), ..said(3, "m2", None, vec![text("Mine.")]) };
    let mut tail = streaming("m3", &[(0, "Its.")]);
    tail.messages[0].subagent = Some("toolu-1".into());

    assert_eq!(reading.recorded(&typed), []);
    assert_eq!(reading.recorded(&subagent), []);
    assert_eq!(reading.streaming(&tail), []);
}

/// What was under way when reading began isn't read, since nobody was listening here when it began, and is said not
/// to have been read out once it's recorded. Its line isn't said either.
#[test]
fn what_was_under_way_when_reading_began_is_not_read_out() {
    let mut under_way = streaming("m1", &[(0, "Half a sen")]);
    under_way.covering = covered(1, 1, "Hold on.").covering;
    let mut reading = Reading::from(&under_way);

    assert_eq!(reading.streaming(&streaming("m1", &[(0, "Half a sentence.")])), []);
    assert_eq!(reading.streaming(&under_way), [], "the line said before");
    assert_eq!(
        reading.recorded(&said(2, "m1", Some(0), vec![text("Half a sentence.")])),
        [heard("m1", Ending::NotReadOut)]
    );
    assert!(!reading.reading());
}

/// Talking over a reply stops the voice, and the reply ends where the voice had got to. Every reply after it isn't
/// read out until the person's next prompt is taken, and is said not to have been once it's recorded.
#[test]
fn talking_over_a_reply_ends_it_where_it_was_heard_and_skips_the_rest_until_the_next_prompt() {
    let mut reading = Reading::from(&Tail::default());
    reading.recorded(&said(2, "m1", None, vec![text("One. Two. Three.")]));
    reading.voice(voice_heard(1, 4));

    assert_eq!(reading.talk(), [Do::Voice(ToVoice::Stop)]);
    let stopped = FromVoice::Stopped { cut: Some((VoiceText(1), 9)), never_started: vec![] };
    assert_eq!(reading.voice(stopped), [heard("m1", Ending::TalkedOver { through: place(0, 9) })]);

    assert_eq!(reading.streaming(&streaming("m2", &[(0, "Four.")])), []);
    assert_eq!(reading.recorded(&said(3, "m2", Some(0), vec![text("Four.")])), [heard("m2", Ending::NotReadOut)]);

    reading.let_go(true);
    reading.recorded(&in_mode(4, EventBody::PromptTaken { prompt: Seq::new(3) }));
    assert_eq!(reading.recorded(&said(5, "m3", None, vec![text("Yes.")])), [say(2, 0, "Yes."), end(2)]);
}

/// Stop ends each reply under way as stopped where it was heard, or not read out where none of it was. One still
/// streaming is said once it's recorded, since the hub keeps only what it has.
#[test]
fn a_stop_ends_each_reply_under_way_once_it_is_recorded() {
    let mut reading = Reading::from(&Tail::default());
    reading.recorded(&said(2, "m1", None, vec![text("One.")]));
    reading.voice(voice_heard(1, 4));
    let mut tail = streaming("m2", &[(0, "Two")]);
    reading.streaming(&tail);

    reading.stop();
    let stopped = FromVoice::Stopped { cut: Some((VoiceText(1), 4)), never_started: vec![VoiceText(2)] };
    assert_eq!(reading.voice(stopped), [heard("m1", Ending::Stopped { through: place(0, 4) })]);

    tail.messages[0].blocks[0].text = "Two more.".into();
    assert_eq!(reading.streaming(&tail), [], "nothing more of it is read");
    assert_eq!(reading.recorded(&said(3, "m2", Some(0), vec![text("Two more.")])), [heard("m2", Ending::NotReadOut)]);
}

/// The voice reads a text first given after it stopped, so what the reading gave it meanwhile, which the voice didn't
/// count among what it stopped, has it stop again. A voice that stopped by failing ends each reply as stopped.
#[test]
fn what_was_given_after_the_voice_stopped_is_stopped_again() {
    let mut reading = Reading::from(&Tail::default());
    reading.recorded(&said(2, "m1", None, vec![text("One.")]));
    reading.voice(voice_heard(1, 4));
    reading.recorded(&said(3, "m2", None, vec![text("Two.")]));

    let failed = FromVoice::Stopped { cut: Some((VoiceText(1), 4)), never_started: vec![] };
    assert_eq!(
        reading.voice(failed),
        [
            heard("m1", Ending::Stopped { through: place(0, 4) }),
            heard("m2", Ending::NotReadOut),
            Do::Voice(ToVoice::Stop)
        ]
    );
}

/// With nothing being read, as once the voice has finished, talking stops nothing. A reply that comes while the
/// person talks isn't read over them, nor one before what they said is taken, since they've moved on, and nor is a line
/// said for the agent while they talk.
#[test]
fn nothing_is_read_over_the_person_talking() {
    let mut reading = Reading::from(&Tail::default());
    reading.recorded(&said(2, "m1", None, vec![text("One.")]));
    reading.voice(finished(1));

    assert_eq!(reading.talk(), []);
    assert_eq!(reading.streaming(&covered(1, 1, "Hold on.")), [], "no line over them");
    assert_eq!(
        reading.recorded(&said(3, "m2", None, vec![text("Two.")])),
        [heard("m1", Ending::ToTheEnd), heard("m2", Ending::NotReadOut)]
    );
    reading.let_go(true);
    assert_eq!(reading.streaming(&covered(1, 2, "Still on it.")), [say(2, 0, "Still on it."), end(2)]);
    assert_eq!(reading.recorded(&said(4, "m3", None, vec![text("Three.")])), [heard("m3", Ending::NotReadOut)]);

    reading.recorded(&in_mode(5, EventBody::PromptTaken { prompt: Seq::new(3) }));
    assert_eq!(reading.recorded(&said(6, "m4", None, vec![text("Four.")])), [say(3, 0, "Four."), end(3)]);
}

/// Talking and then dropping what was said, so nothing is sent, has the agent's next reply read, as nothing new is
/// coming for it to answer.
#[test]
fn dropping_what_was_said_reads_the_next_reply() {
    let mut reading = Reading::from(&Tail::default());

    reading.talk();
    reading.let_go(false);
    assert_eq!(reading.recorded(&said(2, "m1", None, vec![text("One.")])), [say(1, 0, "One."), end(1)]);
}

/// A text block that has nothing in it yet, as when a delta starts one, isn't a text to the voice, which would never
/// say it had finished it, so the voice isn't said to be reading, and the reply is heard to its end without it.
#[test]
fn a_block_with_no_text_is_nothing_to_read() {
    let mut reading = Reading::from(&Tail::default());

    assert_eq!(reading.streaming(&streaming("m1", &[(0, "")])), []);
    assert!(!reading.reading());
    assert_eq!(reading.recorded(&said(2, "m1", Some(0), vec![text(""), call()])), [heard("m1", Ending::ToTheEnd)]);
}

/// A recorded block that differs from what streamed, though it's longer, gives the voice nothing more.
#[test]
fn a_longer_recorded_block_that_differs_from_what_streamed_gives_nothing_more() {
    let mut reading = Reading::from(&Tail::default());
    reading.streaming(&streaming("m1", &[(0, "Sure.")]));

    assert_eq!(reading.recorded(&said(2, "m1", Some(0), vec![text("No, I won't.")])), [end(1)]);
}

/// A voice that can't read a text yet, as while its files are fetched, has its reply end where it was heard, or as
/// not read out, without the rest going quiet.
#[test]
fn what_the_voice_cannot_read_is_not_read_out() {
    let mut reading = Reading::from(&Tail::default());
    reading.recorded(&said(2, "m1", None, vec![text("One.")]));

    assert_eq!(reading.voice(FromVoice::Unread(VoiceText(1))), [heard("m1", Ending::NotReadOut)]);
    assert!(!reading.reading());
    assert_eq!(reading.recorded(&said(3, "m2", None, vec![text("Two.")])), [say(2, 0, "Two."), end(2)]);
}

/// A line said for the agent is no reply, so the bar shows the line while the voice reads only that.
#[test]
fn a_line_is_read_but_is_no_reply() {
    let mut reading = Reading::from(&Tail::default());

    reading.streaming(&covered(1, 1, "Hold on."));
    assert!(reading.reading() && !reading.reading_a_reply());
    reading.recorded(&said(2, "m1", None, vec![text("One.")]));
    assert!(reading.reading_a_reply());
}

/// Each line said for the agent while it can't answer is read once, however often the tail is sent again, and is no
/// reply, so the hub hears nothing of it. A stop doesn't cut a line given after it.
#[test]
fn each_cover_line_is_read_once() {
    let mut reading = Reading::from(&Tail::default());

    assert_eq!(reading.streaming(&covered(1, 1, "Hold on.")), [say(1, 0, "Hold on."), end(1)]);
    assert_eq!(reading.streaming(&covered(1, 1, "Hold on.")), []);
    assert!(reading.reading());
    assert_eq!(reading.voice(voice_heard(1, 8)), []);
    assert_eq!(reading.streaming(&covered(1, 2, "Still on it.")), [say(2, 0, "Still on it."), end(2)]);

    reading.voice(finished(1));
    reading.voice(finished(2));
    assert!(!reading.reading());
}
