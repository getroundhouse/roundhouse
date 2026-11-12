//! Where the transcript makes its text views, which keeps the state of each long one, so its row has its height
//! before it's scrolled to.
//!
//! The kit's text view reads text of more than [`AT_ONCE`] bytes on another thread, and until that's done it draws
//! nothing and has no height. Left to itself it keeps what it read only while it's on screen. So the list measured a
//! long text's row, laid out ahead, as if the text weren't there, scrolled past it in one turn of the wheel, and when
//! the row first showed at the top of the window, the text grew there by all of its length and took the window up to
//! its start.
//!
//! A row makes each text view it draws with [`LongTexts::markdown`], which keeps any long text it's given. A recorded
//! text in a row of its own is drawn by `crate::prose` instead, and with a view from here only until it's parsed. The
//! list draws each loaded row soon after it loads, to measure it, so a long text is read then, once, and its row is
//! measured again whenever what's been read of it changes. Nothing lists ahead of time which texts a row shows, so a
//! text view added to a row can't be left out, and `clippy.toml` disallows the kit's own ways to make one anywhere
//! else.
//!
//! A view draws matches over what it renders, so it keeps any text that has matches of what's being found too, and has
//! it draw them, as `marks` says.
//!
//! A text is kept while the row that last drew it is loaded and still draws it. The text the agent is still writing
//! is drawn with [`LongTexts::streaming`], which keeps it from its first word, so it doesn't go blank as it grows past
//! what's read at once, and what's been read of it carries over when it's recorded, since it keeps its ID. The tail
//! holds what it drew until a recorded row is drawn in its place, as [`LongTexts::draw_row`] says.

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use client_core::session_log::{Row, SessionLog};
use gpui_kit::component::text::{TextView, TextViewState};
use gpui_kit::{App, AppContext as _, ElementId, Entity, SharedString, Subscription, WeakEntity};

use crate::images_apart::images_apart;
use crate::transcript::Transcript;
use marks::Marked;

mod marks;

/// The most the kit's text view reads before it first draws: its `MAX_SYNC_FULL_REPLACE_BYTES`. It goes by the length
/// of the text it's given. The kit doesn't say what it is, so a test checks it hasn't changed. Keeping every text
/// instead would leave nothing to copy, but a kept text view holds tens of KB however short its text is.
const AT_ONCE: usize = 4 * 1024;

/// The text views one transcript keeps, by their IDs. Cheap to clone.
#[derive(Clone, Default)]
pub(crate) struct LongTexts(Rc<RefCell<Texts>>);

#[derive(Default)]
struct Texts {
    kept: HashMap<ElementId, Kept>,
    /// What the row being drawn has drawn so far, for [`LongTexts::draw_row`] to act on once it's done.
    drawn: Vec<Drawn>,
    /// The texts with matches of what's being found, by the IDs of the views they're drawn in, which keep them.
    marked: HashMap<ElementId, Marked>,
}

struct Kept {
    state: Entity<TextViewState>,
    /// What it was last given, which a row drawing other text under its ID brings it up to.
    text: String,
    /// The row that last drew it, which is measured again whenever what's been read of it changes. `None` once that
    /// row has been drawn without it.
    row: Rc<Cell<Option<usize>>>,
    _watch: Subscription,
}

/// A text a row drew under an ID.
struct Drawn {
    id: ElementId,
    /// What to keep under the ID: a text that isn't kept yet, or other text than what's kept. `None` to go on as it
    /// is.
    text: Option<String>,
}

/// Which texts a row's drawing keeps.
#[derive(Clone, Copy)]
enum Keep {
    /// Those over [`AT_ONCE`], which the kit reads on another thread.
    Long,
    /// Any with words in it. One with none is never read, as a thought can end with none.
    Written,
}

impl LongTexts {
    /// `text` as Markdown under `id`, which a row draws it by from one frame to the next. One over [`AT_ONCE`] is
    /// kept, from the end of its row's drawing.
    pub(crate) fn markdown(&self, id: ElementId, text: &str) -> TextView {
        self.view(id, text, Keep::Long)
    }

    /// The text the agent is still writing, as Markdown under `id`, kept from its first word, so the text view it
    /// grows in is the one it's drawn in once it's recorded under the same ID.
    pub(crate) fn streaming(&self, id: ElementId, text: &str) -> TextView {
        self.view(id, text, Keep::Written)
    }

    /// The view kept under `id`, brought up to `text`, if one's kept: one the agent wrote `text` into, which its
    /// recorded row draws until it has a parse of its own.
    pub(crate) fn kept(&self, id: ElementId, text: &str) -> Option<TextView> {
        let kept = self.0.borrow().kept.contains_key(&id);
        kept.then(|| self.view(id, text, Keep::Written))
    }

    fn view(&self, id: ElementId, text: &str, keep: Keep) -> TextView {
        let (text, long) = as_drawn(text);
        let mut texts = self.0.borrow_mut();
        if let Some(kept) = texts.kept.get(&id) {
            let view = TextView::new(&kept.state);
            let text = (kept.text != *text).then(|| text.into_owned());
            texts.drawn.push(Drawn { id, text });
            return view.selectable(true);
        }
        let keeps = texts.marked.contains_key(&id)
            || match keep {
                Keep::Long => long,
                Keep::Written => !text.is_empty(),
            };
        if !keeps {
            return kit_view(id, &text);
        }
        texts.drawn.push(Drawn { id: id.clone(), text: Some(text.to_string()) });
        // Only for this frame, since the text view that's kept is made once the row is drawn. A long text draws
        // nothing until it's read anyway, and given nothing, the kit doesn't read it a second time.
        kit_view(id, if long { "" } else { &text })
    }

    /// Draws `row` with `draw`, then keeps what it drew as [`LongTexts::markdown`] and [`LongTexts::streaming`] say,
    /// and notes that what the row drew last time and doesn't now is in no row.
    ///
    /// The streaming tail, which `tail` says `row` is, goes on holding what it drew. A reply that's done streaming
    /// can leave the tail before its record arrives, since the hub sends the end of a stream as soon as it comes and
    /// a record once it's stored. The record is drawn in the row the tail was in, under the same ID, so what's been
    /// read of it carries over, rather than being read again from nothing.
    pub(crate) fn draw_row<T>(
        &self,
        row: usize,
        tail: bool,
        this: &WeakEntity<Transcript>,
        cx: &mut App,
        draw: impl FnOnce(&mut App) -> T,
    ) -> T {
        let stray = std::mem::take(&mut self.0.borrow_mut().drawn);
        debug_assert!(stray.is_empty(), "a text view was made outside a row, and isn't kept");
        let drawn_row = draw(cx);
        let drawn = std::mem::take(&mut self.0.borrow_mut().drawn);
        for kept in self.0.borrow().kept.values().filter(|kept| !tail && kept.row.get() == Some(row)) {
            kept.row.set(None);
        }
        for Drawn { id, text } in drawn {
            match text {
                Some(text) => self.keep(id, row, text, this, cx),
                None => {
                    if let Some(kept) = self.0.borrow().kept.get(&id) {
                        kept.row.set(Some(row));
                    }
                }
            }
        }
        drawn_row
    }

    /// Lets go of each text that's in no row, or only in one that isn't loaded. The row after the log's is the
    /// streaming tail, which always is.
    pub(crate) fn let_go(&self, log: &SessionLog) {
        let rows = log.row_count();
        let loaded = |row: usize| row == rows || (row < rows && !matches!(log.row(row), Row::Unloaded));
        self.0.borrow_mut().kept.retain(|_, kept| kept.row.get().is_some_and(loaded));
    }

    /// Whether every text that's kept has been read up to what it was last given, so each row has the height it
    /// will keep.
    #[cfg(feature = "testing")]
    pub(crate) fn all_read(&self, cx: &App) -> bool {
        self.0.borrow().kept.values().all(|kept| read_up_to(&kept.state, cx) == kept.text.len())
    }

    /// What each text view that's kept has read, which is what it shows.
    #[cfg(feature = "testing")]
    pub(crate) fn read(&self, cx: &App) -> Vec<String> {
        let texts = self.0.borrow();
        texts.kept.values().map(|kept| kept.state.read(cx).rendered_text().source().to_string()).collect()
    }

    /// Keeps `text` under `id`, drawn in `row`, or brings what's kept there up to it.
    fn keep(&self, id: ElementId, row: usize, text: String, this: &WeakEntity<Transcript>, cx: &mut App) {
        let kept = self.0.borrow().kept.get(&id).map(|kept| (kept.state.clone(), kept.row.clone()));
        if let Some((state, drawn_in)) = kept {
            drawn_in.set(Some(row));
            // It appends what extends the text it has, so what's been read of it stays.
            state.update(cx, |state, cx| state.set_text(&text, cx));
            if let Some(kept) = self.0.borrow_mut().kept.get_mut(&id) {
                kept.text = text;
            }
            self.text_changed(&id);
            return;
        }
        #[expect(clippy::disallowed_methods, reason = "a text view's state is made here, and only here")]
        let state = cx.new(|cx| TextViewState::markdown(&text, cx));
        let row = Rc::new(Cell::new(Some(row)));
        // Its height changes as each reading of it lands, which changes the source it was read from. That's how
        // far a text that's only ever extended has been read, and it builds nothing to find. It says it changed for
        // a selection too, and a fade, which don't change its height.
        let read = Cell::new(read_up_to(&state, cx));
        let watch = cx.observe(&state, {
            let (row, this, texts, watched) = (row.clone(), this.clone(), Rc::downgrade(&self.0), id.clone());
            move |state, cx| {
                let now = read_up_to(&state, cx);
                if read.replace(now) == now {
                    return;
                }
                if let Some(texts) = Weak::upgrade(&texts) {
                    LongTexts(texts).newly_read(&watched, cx);
                }
                if let Some(row) = row.get() {
                    let _ = this.update(cx, |transcript, cx| transcript.remeasure_row(row, cx));
                }
            }
        });
        let kept = id.clone();
        self.0.borrow_mut().kept.insert(id, Kept { state, text, row, _watch: watch });
        self.newly_read(&kept, cx);
    }
}

/// `text` as the kit's text view is given it, with images one after another apart, as `crate::images_apart` says, and
/// whether it's longer than the view reads at once. Images apart make a text longer than it was written, and the view
/// goes by what it's given, so this does too: a text written a little under [`AT_ONCE`] can be over it as it's drawn.
fn as_drawn(text: &str) -> (Cow<'_, str>, bool) {
    let drawn = images_apart(text);
    let too_long = drawn.len() > AT_ONCE;
    (drawn, too_long)
}

/// How much of what a text view was given it's read, in bytes.
fn read_up_to(state: &Entity<TextViewState>, cx: &App) -> usize {
    state.read(cx).rendered_text().source().len()
}

/// The kit's own text view of `text` under `id`, which keeps what it reads only while it's drawn.
fn kit_view(id: ElementId, text: &str) -> TextView {
    #[expect(clippy::disallowed_methods, reason = "a text view is made here, and only here")]
    let view = TextView::markdown(id, SharedString::from(text.to_string()));
    view.selectable(true)
}

#[cfg(test)]
mod tests {
    use gpui_kit::component::text::TextViewState;
    use gpui_kit::{AppContext as _, TestAppContext};

    use super::{AT_ONCE, as_drawn, read_up_to};

    /// The kit reads at once just as much as [`AT_ONCE`] says, so the texts kept are the ones it reads on another
    /// thread. A kit that read more at once would leave a text between drawn as nothing in its first frame and never
    /// measured again, since what's read of it wouldn't change after. One that read less would leave texts between
    /// unkept, read again from nothing each time they're drawn.
    #[gpui_kit::test]
    fn the_kit_reads_as_much_at_once_as_this_takes_it_to(cx: &mut TestAppContext) {
        let read_at_once = |len: usize, cx: &mut TestAppContext| {
            let text = "x".repeat(len);
            #[expect(
                clippy::disallowed_methods,
                reason = "what the kit does with a text view's state is what's tested"
            )]
            let state = cx.new(|cx| TextViewState::markdown(&text, cx));
            cx.read(|cx| read_up_to(&state, cx))
        };

        assert_eq!(read_at_once(AT_ONCE, cx), AT_ONCE, "read at once");
        assert_eq!(read_at_once(AT_ONCE + 1, cx), 0, "read on another thread");
    }

    #[test]
    fn a_text_is_long_by_its_length_with_its_images_apart() {
        let images = "\n\n![Before](before.png)\n![After](after.png)";
        let written = "x".repeat(AT_ONCE - images.len()) + images;
        let plain = "x".repeat(AT_ONCE);

        let (drawn, too_long) = as_drawn(&written);

        assert_eq!((written.len(), drawn.len()), (AT_ONCE, AT_ONCE + 1));
        assert!(too_long, "a text only longer than what's read at once with its images apart is long");
        assert!(!as_drawn(&plain).1, "a text of exactly what's read at once is read at once");
    }
}
