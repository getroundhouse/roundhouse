//! Hearing one recording: live text as it's said, and each sentence's final text once it has ended.
//!
//! A recording is cut into pieces at its pauses, and a pause can come anywhere: where a sentence ends, where a comma
//! would go, or in the middle of a thought. A piece heard whole on its own is heard as a sentence of its own, with a
//! capital and a full stop, and its first and last words without what's either side of them. So pieces are heard
//! whole together, a sentence at a time, and a sentence ends where the live text ends one, as `joins.rs` has it. A
//! sentence too long for the engine to hear whole is cut at a pause, or between words, and joined to the rest of it as
//! the live text joins them.

use std::collections::VecDeque;

use super::buffer::Buffer;
use super::joins::{self, Ends, Join, is_mark};
use super::recording::{Cancelled, Input, Update};
use crate::pauses::{Cut, Pauses};
use crate::{DictationError, Ending, Engine, Heard, Hints, Language, Listening, Samples};

/// Hears the recording `input` brings until it's stopped, which gives how it went, or cancelled, which gives `None`.
pub(super) fn hear(
    engine: &dyn Engine,
    hints: &Hints,
    language: Language,
    input: &Buffer,
    updates: &flume::Sender<Update>,
    cancelled: &Cancelled,
) -> Result<Option<Ending>, DictationError> {
    let mut hearing = Hearing {
        engine,
        hints,
        language,
        listening: engine.listen(hints, language),
        pauses: Pauses::new(engine.longest_piece()),
        ended: VecDeque::new(),
        audio: Vec::new(),
        start: Samples(0),
        live: Vec::new(),
        shown: String::new(),
        said: false,
        carries_on: false,
        updates,
        cancelled,
    };
    loop {
        let next = input.next();
        // Whatever was queued before the cancel goes unheard.
        if cancelled.is_set() {
            return Ok(None);
        }
        match next {
            Input::Audio(audio) => hearing.audio(&audio)?,
            Input::Stop => return hearing.stop().map(Some),
            Input::Failed(e) => return Err(e),
            // Its handle is gone, so nobody is left to hear it.
            Input::Cancel => return Ok(None),
        }
    }
}

struct Hearing<'e> {
    engine: &'e dyn Engine,
    hints: &'e Hints,
    language: Language,
    listening: Box<dyn Listening + 'e>,
    pauses: Pauses,
    /// Pieces that have ended but aren't final yet, in order: those of a sentence that hasn't ended, or whose end isn't
    /// known, ones with no speech by their level, until the live text has been given past their end, in case it finds
    /// words in them, and any after them.
    ended: VecDeque<Ended>,
    /// The audio of the piece that hasn't ended, from `start`.
    audio: Vec<f32>,
    start: Samples,
    /// What's been heard live of the piece that hasn't ended.
    live: Vec<Heard>,
    /// The live text last sent.
    shown: String,
    /// Whether any final text has been sent.
    said: bool,
    /// Whether the next final text begins in lower case: the last was a sentence cut short, and the live text has the
    /// word after the cut in lower case.
    carries_on: bool,
    updates: &'e flume::Sender<Update>,
    cancelled: &'e Cancelled,
}

/// A piece of the recording that has ended.
struct Ended {
    /// Its audio, which goes once it's known nothing was said in it.
    audio: Vec<f32>,
    start: Samples,
    end: Samples,
    /// Whether its level said someone spoke in it.
    speech: bool,
    /// What's been heard live of it.
    live: Vec<Heard>,
}

impl Ended {
    /// Whether anything was said in it, by its level or by the words heard in it. Someone speaking quietly enough that
    /// the level doesn't count it can still be heard, and their words still count, but a mark such as `.` isn't a word.
    fn said(&self) -> bool {
        self.speech || self.live.iter().any(|heard| !is_mark(heard))
    }

    /// Whether anything was said in it, once that's known from what live text has been `given`: at once when it was,
    /// and when it wasn't, once the live text has been given past its end.
    fn known_said(&self, given: Given) -> Option<bool> {
        match given {
            _ if self.said() => Some(true),
            Given::All => Some(false),
            Given::Through(through) => (through >= self.end).then_some(false),
        }
    }
}

/// How much of the recording's live text has been given.
#[derive(Clone, Copy)]
enum Given {
    /// All that was said before this place in the recording.
    Through(Samples),
    /// All of it, as the recording has ended.
    All,
}

impl Hearing<'_> {
    fn audio(&mut self, audio: &[f32]) -> Result<(), DictationError> {
        let heard = self.listening.hear(audio)?;
        self.take(heard);
        self.audio.extend_from_slice(audio);
        for cut in self.pauses.hear(audio) {
            self.end_piece(cut);
        }
        self.finish_sentences(Given::Through(self.listening.heard_through()))?;
        self.show();
        Ok(())
    }

    fn stop(mut self) -> Result<Ending, DictationError> {
        let held = self.listening.finish()?;
        self.take(held);
        let end = self.start + Samples(self.audio.len());
        self.end_piece(Cut { at: end, speech: self.pauses.speech() });
        // Text given as said at the very end, or past it, is the last piece's, since nothing follows it.
        let past_the_end = std::mem::take(&mut self.live);
        if let Some(last) = self.ended.back_mut() {
            last.live.extend(past_the_end);
        }
        self.finish_sentences(Given::All)?;
        self.show();
        Ok(if self.said { Ending::Heard } else { Ending::NothingHeard })
    }

    /// Puts what's heard live with the piece it was said in, unless that piece is final already, when its words are
    /// in its final text.
    fn take(&mut self, heard: Vec<Heard>) {
        for heard in heard {
            if heard.at >= self.start {
                self.live.push(heard);
            } else if let Some(piece) =
                self.ended.iter_mut().find(|piece| piece.start <= heard.at && heard.at < piece.end)
            {
                piece.live.push(heard);
            }
        }
    }

    /// Ends the piece that hasn't ended at `cut`, to be made final once it can be.
    fn end_piece(&mut self, cut: Cut) {
        let audio = self.audio.drain(..(cut.at - self.start).0).collect();
        let (live, after) = self.live.drain(..).partition(|heard| heard.at < cut.at);
        self.live = after;
        self.ended.push_back(Ended { audio, start: self.start, end: cut.at, speech: cut.speech, live });
        self.start = cut.at;
    }

    /// Sends the final text of each sentence whose pieces have ended, in order, once it's known where it ends from the
    /// live text `given`. Pieces nothing was said in go, once that's known. None of a cancelled recording's is heard.
    fn finish_sentences(&mut self, given: Given) -> Result<(), DictationError> {
        loop {
            if self.cancelled.is_set() {
                return Ok(());
            }
            while self.ended.front().is_some_and(|piece| piece.known_said(given) == Some(false)) {
                self.ended.pop_front();
            }
            // Nothing said in it, it's left out of what's heard whole. Behind a sentence that hasn't ended, a pause can go
            // on for as long as the person thinks, so its audio goes now.
            for piece in self.ended.iter_mut().filter(|piece| piece.known_said(given) == Some(false)) {
                piece.audio = Vec::new();
            }
            let Some((pieces, ends)) = self.next_sentence(given) else { return Ok(()) };
            let pieces: Vec<Ended> = self.ended.drain(..pieces).collect();
            self.finish_sentence(pieces, ends)?;
        }
    }

    /// How many of the pieces at the front make the next sentence, and how it ends, once that's known from the live
    /// text `given`: where the live text ends one, which it often does only once it has heard the next word begin,
    /// where it would be too long to be heard whole with the next piece said, and where the recording does. The first
    /// piece was said, or it isn't known yet whether it was.
    fn next_sentence(&self, given: Given) -> Option<(usize, Ends)> {
        self.ended.front()?.known_said(given)?;
        let (mut last, mut length) = (0, self.ended[0].audio.len());
        loop {
            let join = self.join_after(last);
            let Some(next_word) = join.next else {
                // Nothing has been said since, so far.
                let ends = join.ends_sentence().map_or(Ends::Unmarked, Ends::Marked);
                return matches!(given, Given::All).then_some((last + 1, ends));
            };
            if let Some(mark) = join.ends_sentence() {
                return Some((last + 1, Ends::Marked(mark)));
            }
            // The sentence goes on into the next piece that was said, once that has ended. The word after the join is in
            // it, or in the piece that hasn't ended, which every piece has once the recording has.
            let mut next = None;
            for (i, piece) in self.ended.iter().enumerate().skip(last + 1) {
                if piece.known_said(given)? {
                    next = Some(i);
                    break;
                }
            }
            let next = next?;
            length += self.ended[next].audio.len();
            if length > self.engine.longest_piece().0 {
                return Some((last + 1, Ends::Cut { marks: join.marks, next: next_word }));
            }
            last = next;
        }
    }

    /// How the live text joins the piece `at` to what follows it.
    fn join_after(&self, at: usize) -> Join {
        let after = self.ended.iter().skip(at + 1).flat_map(|piece| &piece.live).chain(&self.live);
        Join::of(&self.ended[at].live, after)
    }

    /// Hears `pieces` whole, as one sentence, and sends its final text, which ends as `ends` says. Pieces between them
    /// that nothing was said in have let go of their audio, so a long pause takes none of the room the engine has.
    /// Heard whole as nothing, as the model hears some short phrases said quickly, it's what the live text heard.
    fn finish_sentence(&mut self, pieces: Vec<Ended>, ends: Ends) -> Result<(), DictationError> {
        let words = pieces.iter().flat_map(|piece| &piece.live).skip_while(|heard| is_mark(heard));
        let live: String = words.map(|heard| heard.text.as_str()).collect();
        let audio: Vec<f32> = pieces.into_iter().flat_map(|piece| piece.audio).collect();
        let heard = self.engine.transcribe(&audio, self.hints, self.language)?;
        let text = if joins::has_words(&heard) { heard } else { live };
        let carries_on = std::mem::take(&mut self.carries_on);
        if let Some((text, next_carries_on)) = joins::final_text(&text, carries_on, ends) {
            self.carries_on = next_carries_on;
            self.said = true;
            self.send(Update::Final(text));
        }
        Ok(())
    }

    /// Sends the live text of everything that isn't final, if it has changed. Marks before its first word join it to
    /// what's final already, which has its own.
    fn show(&mut self) {
        let pieces = self.ended.iter().flat_map(|piece| &piece.live);
        let words = pieces.chain(&self.live).skip_while(|heard| is_mark(heard));
        let live: String = words.map(|heard| heard.text.as_str()).collect();
        let live = live.trim_start();
        if live != self.shown {
            self.shown = live.to_string();
            self.send(Update::Hearing(self.shown.clone()));
        }
    }

    /// Sends `update`, unless the recording has been cancelled since, when nothing more is said of it.
    fn send(&self, update: Update) {
        self.cancelled.send_unless_set(self.updates, update);
    }
}
