//! Reading texts out, one after another, as a plain state the speaking thread drives: what's said, which phrase to make
//! next, how far each text has been heard as its marks come back, and what a stop leaves. It makes no sound and waits
//! on nothing, so it's tested without a voice or speakers.

use std::collections::VecDeque;

use crate::phrases::Cutting;
use crate::speakable::speakable;
use crate::{TextId, Update};

/// How many phrases are made ahead of what's being heard, so the voice never waits for the next, and a stop drops
/// little that was made.
pub(crate) const AHEAD: usize = 3;

/// The texts being read out, in the order they were first given, and the phrases made of them that haven't been heard.
#[derive(Debug, Default)]
pub(crate) struct Reader {
    texts: VecDeque<Text>,
    /// The phrases given to be played whose marks haven't come back, in order.
    playing: VecDeque<Placed>,
    /// The next mark, numbered across every text, so one from before a stop matches nothing after it.
    next_mark: u64,
}

#[derive(Debug)]
struct Text {
    id: TextId,
    cutting: Cutting,
    /// A phrase of it has been given to be played.
    started: bool,
    /// How far into it has been heard, in bytes.
    heard: usize,
}

/// A phrase given to be played, and where it ends in its text.
#[derive(Debug)]
struct Placed {
    mark: u64,
    text: TextId,
    through: usize,
    /// It's the text's last.
    last: bool,
}

/// The next phrase to make and play: what to say of it, which can be nothing, and the mark to put after it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Phrase {
    pub(crate) say: String,
    pub(crate) mark: u64,
}

impl Reader {
    /// Takes the next of `text`, `more`, which starts `at` bytes into it. A text is read after every text given before
    /// it, and starts with what's given at 0. What doesn't follow on from what's been given is dropped, such as the
    /// rest of a text a stop has forgotten, so a place is always counted from the text's start.
    pub(crate) fn say(&mut self, text: TextId, at: usize, more: &str) {
        match self.texts.iter_mut().find(|known| known.id == text) {
            Some(known) if known.cutting.text().len() == at => known.cutting.push(more),
            None if at == 0 => {
                let mut cutting = Cutting::default();
                cutting.push(more);
                self.texts.push_back(Text { id: text, cutting, started: false, heard: 0 });
            }
            Some(_) | None => {}
        }
    }

    /// No more of `text` is coming. A text it isn't reading is left alone.
    pub(crate) fn end(&mut self, text: TextId) {
        if let Some(known) = self.texts.iter_mut().find(|known| known.id == text) {
            known.cutting.end();
        }
    }

    /// Whether it has a text to read, or one it's reading.
    pub(crate) fn is_reading(&self) -> bool {
        !self.texts.is_empty()
    }

    /// Whether a phrase it gave is still to be heard.
    pub(crate) fn is_playing(&self) -> bool {
        !self.playing.is_empty()
    }

    /// The next phrase to make, once all of it has come, unless [`AHEAD`] are already waiting to be heard. A text's
    /// phrases come only once every text before it has been cut whole.
    pub(crate) fn next(&mut self) -> Option<Phrase> {
        if self.playing.len() >= AHEAD {
            return None;
        }
        let text = self.texts.iter_mut().find(|text| !text.cutting.finished())?;
        let piece = text.cutting.next()?;
        text.started = true;
        let say = speakable(&text.cutting.text()[piece.range.clone()], piece.kind);
        let mark = self.next_mark;
        self.next_mark += 1;
        let last = text.cutting.finished();
        self.playing.push_back(Placed { mark, text: text.id, through: piece.range.end, last });
        Some(Phrase { say, mark })
    }

    /// The speakers have played everything before `mark`: how far that has been heard into its text, if further than
    /// before, and that the text was heard to its end, if that was its last phrase. A mark from before a stop says
    /// nothing.
    pub(crate) fn reached(&mut self, mark: u64) -> Vec<Update> {
        let Some(at) = self.playing.iter().position(|placed| placed.mark == mark) else { return Vec::new() };
        let mut updates = Vec::new();
        // Marks come back in order, so any before it were played too.
        for placed in self.playing.drain(..=at) {
            if let Some(text) = self.texts.iter_mut().find(|text| text.id == placed.text)
                && placed.through > text.heard
            {
                text.heard = placed.through;
                updates.push(Update::Heard { text: placed.text, through: placed.through });
            }
            if placed.last {
                self.texts.retain(|text| text.id != placed.text);
                updates.push(Update::Finished { text: placed.text });
            }
        }
        updates
    }

    /// Stops reading: says which text was cut off and how far it had been heard, if one had been started, and which
    /// never were, and forgets them all.
    pub(crate) fn stop(&mut self) -> Update {
        self.playing.clear();
        let texts = std::mem::take(&mut self.texts);
        let cut = texts.front().filter(|text| text.started).map(|text| (text.id, text.heard));
        let never_started = texts.iter().skip(usize::from(cut.is_some())).map(|text| text.id).collect();
        Update::Stopped { cut, never_started }
    }
}
