//! The text views whose state the transcript keeps, so a long message has its height before it's scrolled to.
//!
//! The kit's text view reads text of more than [`AT_ONCE`] bytes on another thread, and until that's done it draws
//! nothing and has no height. Left to itself it keeps what it read only while it's on screen. So the list would
//! measure a long message as a few pixels when it laid the row out ahead, scroll past it in one turn of the wheel,
//! and read it again every time it came back into view. Kept here, a long message is read once, when its row
//! loads, and its row is measured again as soon as that's done.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use client_core::session_log::{Row, SessionLog};
use gpui_kit::component::text::TextViewState;
use gpui_kit::{App, AppContext as _, Entity, Subscription, WeakEntity};
use protocol::model::{Seq, seq_index};
use protocol::tail::Tail;

use crate::item_key::ItemKey;
use crate::rows::{tail_texts, texts_of};
use crate::transcript::Transcript;

/// The most the kit's text view reads before it first draws: its `MAX_SYNC_FULL_REPLACE_BYTES`.
const AT_ONCE: usize = 4 * 1024;

#[derive(Default)]
pub(crate) struct LongTexts {
    held: HashMap<ItemKey, Held>,
}

struct Held {
    state: Entity<TextViewState>,
    /// The event it's recorded in. `None` while it's only in the streaming tail.
    seq: Rc<Cell<Option<Seq>>>,
    _watch: Subscription,
}

impl LongTexts {
    pub(crate) fn get(&self, key: &ItemKey) -> Option<&Entity<TextViewState>> {
        self.held.get(key).map(|held| &held.state)
    }

    /// Keeps the long texts of the events in `rows`, whose content has changed, and the text the agent is still
    /// writing, and lets go of what's no longer in the log.
    pub(crate) fn follow(
        &mut self,
        log: &SessionLog,
        rows: std::ops::Range<usize>,
        this: &WeakEntity<Transcript>,
        cx: &mut App,
    ) {
        self.hold_rows(log, rows, this, cx);
        self.hold_streaming(log.tail(), this, cx);
        let streaming: Vec<ItemKey> = tail_texts(log.tail()).into_iter().map(|(key, _)| key).collect();
        self.held.retain(|key, held| match held.seq.get() {
            Some(seq) => matches!(log.row(seq_index(seq).saturating_sub(1)), Row::Event(_)),
            None => streaming.contains(key),
        });
    }

    /// Whether every long text in a recorded event has been read, so each row has the height it will keep.
    #[cfg(feature = "testing")]
    pub(crate) fn all_read(&self, cx: &App) -> bool {
        let recorded = self.held.values().filter(|held| held.seq.get().is_some());
        recorded.into_iter().all(|held| !held.state.read(cx).rendered_text().is_empty())
    }

    /// Keeps the long texts of the events in `rows`, and of any that were kept while they streamed, which go on
    /// with what they've read.
    fn hold_rows(
        &mut self,
        log: &SessionLog,
        rows: std::ops::Range<usize>,
        this: &WeakEntity<Transcript>,
        cx: &mut App,
    ) {
        for row in rows.start..rows.end.min(log.row_count()) {
            let Row::Event(event) = log.row(row) else { continue };
            for (key, text) in texts_of(event) {
                if text.len() > AT_ONCE || self.held.contains_key(&key) {
                    self.hold(key, Some(event.seq), text, this, cx);
                }
            }
        }
    }

    /// Keeps the text the agent is still writing, from its first word, so what's been read of it carries over when
    /// it's recorded and it doesn't go blank as it grows past what's read at once.
    fn hold_streaming(&mut self, tail: &Tail, this: &WeakEntity<Transcript>, cx: &mut App) {
        for (key, text) in tail_texts(tail) {
            self.hold(key, None, text, this, cx);
        }
    }

    /// Keeps `text` under `key`, or brings what's kept there up to it. `seq` is the event it's recorded in, if it
    /// is yet.
    fn hold(&mut self, key: ItemKey, seq: Option<Seq>, text: &str, this: &WeakEntity<Transcript>, cx: &mut App) {
        if let Some(held) = self.held.get(&key) {
            held.seq.set(seq.or(held.seq.get()));
            held.state.update(cx, |state, cx| state.set_text(text, cx));
            return;
        }
        let state = cx.new(|cx| TextViewState::markdown(text, cx));
        let seq = Rc::new(Cell::new(seq));
        let read = Cell::new(!state.read(cx).rendered_text().is_empty());
        // Once it's been read it has its height, which its row is measured for again. It says it changed for a
        // selection too, and a fade, which don't change its height.
        let watch = cx.observe(&state, {
            let (seq, this) = (seq.clone(), this.clone());
            move |state, cx| {
                if read.get() || state.read(cx).rendered_text().is_empty() {
                    return;
                }
                read.set(true);
                if let Some(seq) = seq.get() {
                    let _ = this.update(cx, |transcript, cx| transcript.remeasure(seq, cx));
                }
            }
        });
        self.held.insert(key, Held { state, seq, _watch: watch });
    }
}
