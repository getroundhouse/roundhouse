//! Which of a chat's replies the device that has its conversation reads out, and what it tells the hub as it does:
//! how far it has read each, and how each ended. It does no IO: it's told what the chat's log and tail hold, and what
//! the voice says, and says what to give the voice and what to tell the hub, in order.
//!
//! A reply is one of the agent's own messages with text in it, recorded while the mode was on, which is what the hub
//! settles as not read out if the device never says how it ended. Each of its text blocks is a text of its own to the
//! voice, given as it streams, so the voice starts on a reply's first phrase, and a place in it is the block and the
//! bytes of the block's text the voice has got through, as [`ReplyPlace`] counts them. What the agent says while it
//! can't answer, the cover line, is read too, and is no reply.

use std::collections::{HashMap, HashSet};

use protocol::event::{ContentBlock, EventBody, SessionEvent};
use protocol::model::{Ending, InteractionMode, MessageId, ReplyPlace, RunId, SpokenThrough};
use protocol::tail::{BlockKind, Tail};

use super::telling::{Do, FromVoice, ToHub, ToVoice, VoiceText};

/// Why reading was stopped, which is how the reply being read ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Why {
    /// The person started talking over it.
    TalkedOver,
    /// Stop, or typing.
    Stopped,
}

/// One of the agent's replies, from when it's first seen until how it ended is said.
#[derive(Debug)]
struct Reply {
    message: MessageId,
    /// Its text blocks, in the order they came.
    blocks: Vec<Block>,
    /// How it ended, once that's known, as for one that had begun before reading did, or was stopped, or came after a
    /// stop, before the person spoke again. Nothing more of it is read, and it's said as soon as it's recorded.
    ending: Option<Ending>,
    /// The agent has gone on past it, so no more of it will come.
    done: bool,
    /// It has been recorded in the mode, so the hub has it to keep how it ended.
    recorded: bool,
    /// The furthest the voice has got through it.
    heard: Option<ReplyPlace>,
}

#[derive(Debug)]
struct Block {
    index: u32,
    text: VoiceText,
    /// What of it the voice has been given.
    given: String,
    ended: bool,
    finished: bool,
}

/// Reading out one chat's replies, from when the device has the conversation until it hasn't.
#[derive(Debug, Default)]
pub struct Reading {
    replies: Vec<Reply>,
    /// Replies whose ending has been said, or that the hub has, so what more comes of them is passed over.
    settled: HashSet<MessageId>,
    /// Each text given to the voice: the reply and block it's of, or `None` for a cover line.
    texts: HashMap<VoiceText, Option<(MessageId, u32)>>,
    next_text: u64,
    /// The cover lines seen, by run, wait and number, so each is said once.
    covers: HashSet<(Option<RunId>, u32, u32)>,
    /// Stopped, or the person talking, until what they said is taken or the agent starts a turn by itself: replies
    /// meanwhile aren't read.
    quiet: bool,
    /// The person is talking, so nothing is said over them, not even a cover line.
    hearing: bool,
    /// Why it was asked to stop, until the voice says it has.
    stopping: Option<Why>,
}

impl Reading {
    /// Reading from now, when the chat's tail holds `tail`: what's under way already isn't read, since it began before
    /// the person was listening here, and is said not to have been read out once it's recorded. A device that had the
    /// conversation before may have read some of it, which the hub keeps rather than this. The line in it has been
    /// said, or not, already.
    pub fn from(tail: &Tail) -> Self {
        let mut reading = Self { quiet: true, ..Self::default() };
        reading.streaming(tail);
        reading.quiet = false;
        reading.texts.clear();
        reading
    }

    /// Takes in an event recorded before reading began. A reply in it isn't read, and nothing is said of it, since
    /// the device that had the conversation then may have said how it ended.
    pub fn before(&mut self, event: &SessionEvent) {
        if let EventBody::Assistant { message_id: Some(message), subagent: None, .. } = &event.body
            && !self.replies.iter().any(|reply| reply.message == *message)
        {
            self.settled.insert(message.clone());
        }
    }

    /// Takes in what the chat's tail holds now: more of a reply as it streams, and the cover line.
    pub fn streaming(&mut self, tail: &Tail) -> Vec<Do> {
        let mut said = Vec::new();
        if let Some(covering) = tail.covering.as_deref() {
            let line = &covering.line;
            // Not over the person, and once is all a line is said.
            if self.covers.insert((covering.run, line.wait.0, line.number)) && !self.hearing && !line.text.is_empty() {
                let text = self.new_text(None);
                said.push(Do::Voice(ToVoice::Say { text, at: 0, more: line.text.clone() }));
                said.push(Do::Voice(ToVoice::End(text)));
            }
        }
        let own = tail.messages.iter().filter(|message| message.subagent.is_none());
        for message in own {
            if self.settled.contains(&message.message_id) {
                continue;
            }
            let reply = self.reply(&message.message_id);
            for block in message.blocks.iter().filter(|block| block.kind == BlockKind::Text) {
                self.give(reply, block.index, &block.text, false, &mut said);
            }
        }
        said
    }

    /// Takes in an event recorded in the chat, after every event seen before it.
    pub fn recorded(&mut self, event: &SessionEvent) -> Vec<Do> {
        let mut said = Vec::new();
        let body = &event.body;
        if event.subagent.is_some() {
            return said;
        }
        if let EventBody::Assistant { message_id: Some(message), subagent: None, blocks, first_index, .. } = body {
            self.finish_others(Some(message), &mut said);
            if event.interaction == InteractionMode::Conversation && !self.settled.contains(message) {
                self.written(message, blocks, first_index.unwrap_or(0), &mut said);
            }
        } else if matches!(
            body,
            EventBody::AgentUser { subagent: None, .. } | EventBody::TurnEnded { .. } | EventBody::AgentExited { .. }
        ) {
            self.finish_others(None, &mut said);
        } else if matches!(body, EventBody::PromptTaken { .. } | EventBody::OwnTurn(_)) {
            self.quiet = false;
        }
        said
    }

    /// The agent's message `message` recorded `blocks`, the first of them its block `first`, in the mode.
    fn written(&mut self, message: &MessageId, blocks: &[ContentBlock], first: u32, said: &mut Vec<Do>) {
        let mut texts = Vec::new();
        for (index, block) in (first..).zip(blocks) {
            if let ContentBlock::Text { text, .. } = block {
                texts.push((index, text.as_str()));
            }
        }
        // A message's calls come after its text, so once one is recorded, no more of its text will come.
        let calls = blocks.iter().any(|block| matches!(block, ContentBlock::Call { .. }));
        let under_way = self.replies.iter().position(|reply| reply.message == *message);
        let reply = match under_way {
            Some(reply) => reply,
            None if texts.is_empty() => return,
            None => self.reply(message),
        };
        self.replies[reply].recorded |= !texts.is_empty();
        for (index, text) in texts {
            self.give(reply, index, text, true, said);
        }
        if calls {
            self.replies[reply].done = true;
            self.finish_others(None, said);
        }
        self.settle_ended(said);
    }

    /// Stops reading, as Stop or typing does. Each reply under way ends as the voice says it stopped, and the replies
    /// after it, until what the person says next is taken, aren't read out. With nothing being read, it does nothing.
    pub fn stop(&mut self) -> Vec<Do> {
        if !self.reading() {
            return Vec::new();
        }
        self.quiet = true;
        self.stop_for(Why::Stopped)
    }

    /// The person has started talking. What's being read is talked over, and nothing is read over them: a reply that
    /// comes meanwhile, or before what they say is taken, isn't read out, since they've moved on.
    pub fn talk(&mut self) -> Vec<Do> {
        self.hearing = true;
        self.quiet = true;
        if self.reading() { self.stop_for(Why::TalkedOver) } else { Vec::new() }
    }

    /// The person has stopped talking, having `sent` what they said, or dropped it. Once dropped, the agent's next
    /// reply is read, as nothing new is coming for it to answer.
    pub fn let_go(&mut self, sent: bool) {
        self.hearing = false;
        if !sent {
            self.quiet = false;
        }
    }

    fn stop_for(&mut self, why: Why) -> Vec<Do> {
        self.stopping = Some(why);
        vec![Do::Voice(ToVoice::Stop)]
    }

    /// Whether the voice has something of a reply or a cover line it hasn't finished.
    pub fn reading(&self) -> bool {
        !self.texts.is_empty()
    }

    /// Whether the voice has something of a reply it hasn't finished, rather than only a cover line.
    pub fn reading_a_reply(&self) -> bool {
        self.texts.values().any(Option::is_some)
    }

    /// Takes in what the voice said.
    pub fn voice(&mut self, from: FromVoice) -> Vec<Do> {
        let mut said = Vec::new();
        match from {
            FromVoice::Heard { text, through } => {
                if let Some((reply, block)) = self.of_text(text) {
                    let place = ReplyPlace { block, through: through.try_into().unwrap_or(u32::MAX) };
                    let reply = &mut self.replies[reply];
                    reply.heard = Some(place);
                    let through = SpokenThrough { message: reply.message.clone(), through: place };
                    said.push(Do::Hub(ToHub::Speaking(through)));
                }
            }
            FromVoice::Finished(text) => {
                if let Some((reply, block)) = self.of_text(text)
                    && let Some(block) = self.replies[reply].blocks.iter_mut().find(|b| b.index == block)
                {
                    block.finished = true;
                }
                self.texts.remove(&text);
                self.settle_ended(&mut said);
            }
            FromVoice::Stopped { cut, never_started } => self.stopped(cut, never_started, &mut said),
            FromVoice::Unread(text) => {
                if let Some((reply, _)) = self.of_text(text) {
                    let reply = &mut self.replies[reply];
                    reply.ending = Some(reply.heard.map_or(Ending::NotReadOut, |through| Ending::Stopped { through }));
                }
                self.texts.remove(&text);
                self.settle_ended(&mut said);
            }
        }
        said
    }

    /// The voice stopped: every reply under way ends where it had got to, and is said as soon as it's recorded, and
    /// anything of a reply given that the voice didn't count among what it stopped was given after, so it would be
    /// read, and is stopped again.
    fn stopped(&mut self, cut: Option<(VoiceText, usize)>, never_started: Vec<VoiceText>, said: &mut Vec<Do>) {
        let why = self.stopping.take().unwrap_or(Why::Stopped);
        self.quiet = true;
        if let Some((text, through)) = cut
            && let Some((reply, block)) = self.of_text(text)
            && through > 0
        {
            let place = ReplyPlace { block, through: through.try_into().unwrap_or(u32::MAX) };
            let heard = &mut self.replies[reply].heard;
            *heard = Some(heard.map_or(place, |heard| heard.max(place)));
        }
        let counted: HashSet<VoiceText> = cut.map(|(text, _)| text).into_iter().chain(never_started).collect();
        let left = self.texts.iter().any(|(text, of)| !counted.contains(text) && of.is_some());
        // A line given after the stop is read, as a line always is.
        self.texts.retain(|text, of| of.is_none() && !counted.contains(text));
        for reply in self.replies.iter_mut().filter(|reply| reply.ending.is_none()) {
            reply.ending = Some(match reply.heard {
                Some(through) if why == Why::TalkedOver => Ending::TalkedOver { through },
                Some(through) => Ending::Stopped { through },
                None => Ending::NotReadOut,
            });
        }
        self.settle_ended(said);
        if left {
            said.push(Do::Voice(ToVoice::Stop));
        }
    }

    /// The reply `message`, seen for the first time unless it's under way, and not read out while it's quiet.
    fn reply(&mut self, message: &MessageId) -> usize {
        if let Some(at) = self.replies.iter().position(|reply| reply.message == *message) {
            return at;
        }
        self.replies.push(Reply {
            message: message.clone(),
            blocks: Vec::new(),
            ending: self.quiet.then_some(Ending::NotReadOut),
            done: false,
            recorded: false,
            heard: None,
        });
        self.replies.len() - 1
    }

    /// Gives the voice what's new of the block `index` of `reply`, which holds `text` so far, and ends it once it's
    /// `whole`. What doesn't follow on from what was given, as a recorded block that differs from what streamed,
    /// isn't given: the block ends where it is. A block is a text to the voice only once it has some, since the voice
    /// knows nothing of one it was never given, and would never say it had finished it.
    fn give(&mut self, reply: usize, index: u32, text: &str, whole: bool, said: &mut Vec<Do>) {
        if self.replies[reply].ending.is_some() {
            return;
        }
        let at = if let Some(at) = self.replies[reply].blocks.iter().position(|block| block.index == index) {
            at
        } else if text.is_empty() {
            return;
        } else {
            let message = self.replies[reply].message.clone();
            let voice = self.new_text(Some((message, index)));
            let block = Block { index, text: voice, given: String::new(), ended: false, finished: false };
            self.replies[reply].blocks.push(block);
            self.replies[reply].blocks.len() - 1
        };
        let block = &mut self.replies[reply].blocks[at];
        if block.ended {
            return;
        }
        if let Some(more) = text.strip_prefix(block.given.as_str()).filter(|more| !more.is_empty()) {
            said.push(Do::Voice(ToVoice::Say { text: block.text, at: block.given.len(), more: more.into() }));
            block.given = text.into();
        }
        if whole {
            block.ended = true;
            said.push(Do::Voice(ToVoice::End(block.text)));
        }
    }

    /// The agent has gone on past every reply but `message`: none of them gets more, so each ends where it is.
    fn finish_others(&mut self, message: Option<&MessageId>, said: &mut Vec<Do>) {
        let others = self.replies.iter_mut().filter(|reply| Some(&reply.message) != message && reply.ending.is_none());
        for reply in others {
            reply.done = true;
            for block in reply.blocks.iter_mut().filter(|block| !block.ended) {
                block.ended = true;
                said.push(Do::Voice(ToVoice::End(block.text)));
            }
        }
        self.settle_ended(said);
    }

    /// Says how each reply ended that the hub has to keep it: one read to its end once the agent has gone on past it,
    /// and one that wasn't, or was cut off, as soon as it's recorded. Nothing more of it is read.
    fn settle_ended(&mut self, said: &mut Vec<Do>) {
        let (ended, under_way): (Vec<Reply>, Vec<Reply>) =
            std::mem::take(&mut self.replies).into_iter().partition(|reply| {
                let heard = reply.done && reply.blocks.iter().all(|block| block.finished);
                reply.recorded && (reply.ending.is_some() || heard)
            });
        self.replies = under_way;
        for reply in ended {
            self.texts.retain(|_, of| of.as_ref().is_none_or(|(message, _)| *message != reply.message));
            let ending = reply.ending.unwrap_or(Ending::ToTheEnd);
            said.push(Do::Hub(ToHub::Heard { message: reply.message.clone(), ending }));
            self.settled.insert(reply.message);
        }
    }

    fn new_text(&mut self, of: Option<(MessageId, u32)>) -> VoiceText {
        self.next_text += 1;
        let text = VoiceText(self.next_text);
        self.texts.insert(text, of);
        text
    }

    /// The reply, by where it is among those under way, and the block `text` is of.
    fn of_text(&self, text: VoiceText) -> Option<(usize, u32)> {
        let (message, block) = self.texts.get(&text)?.as_ref()?;
        let reply = self.replies.iter().position(|reply| reply.message == *message)?;
        Some((reply, *block))
    }
}
