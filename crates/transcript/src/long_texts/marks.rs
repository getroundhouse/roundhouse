//! The matches of what's being found in Markdown, which the kit's text view draws over what it renders. So a text with
//! matches is kept, as [`LongTexts::mark`] has it, and once its view has read all of what it was given, its matches are
//! worked out in what it renders and drawn, and the one being shown is brought into view when it's in it.

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::component::text::{RangeHighlight, TextViewState};
use gpui_kit::{App, ElementId, Entity};
use protocol::find::Needle;

use super::LongTexts;
use crate::find::{FindLook, TextId, markdown, view_id};

/// What a kept view draws of what's being found.
pub(super) struct Marked {
    text: TextId,
    needle: Needle,
    /// Which of its matches is the one being shown, if it's one of them.
    here: Option<usize>,
    /// Where they are in what it renders, once it's read all of what it was given.
    ranges: Option<Rc<[Range<usize>]>>,
    reveal: Reveal,
}

/// Whether a view is to bring the match being shown into view.
/// Where the needle is in what a view renders, and which of those is the match being shown, if one of them is.
struct Placed {
    ranges: Rc<[Range<usize>]>,
    here: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Reveal {
    No,
    /// Once it's read its text.
    Wanted,
    /// It's been asked to.
    Asked,
}

impl LongTexts {
    /// Has the view of `text`, which has matches of what `find` looks for, draw them. It's kept from the end of the
    /// drawing of the row that draws it, and draws them once it's read it.
    pub(crate) fn mark(&self, text: &TextId, find: &FindLook, cx: &mut App) {
        let Some(needle) = find.needle() else { return };
        let (id, here) = (view_id(text), find.nth_here(text));
        {
            let mut texts = self.0.borrow_mut();
            match texts.marked.get_mut(&id) {
                Some(marked) if marked.needle == *needle && marked.here == here => return,
                Some(marked) if marked.needle == *needle => marked.here = here,
                Some(_) | None => {
                    let (text, needle) = (text.clone(), needle.clone());
                    texts.marked.insert(id.clone(), Marked { text, needle, here, ranges: None, reveal: Reveal::No });
                }
            }
        }
        self.draw_marks(&id, cx);
    }

    /// Has each view with matches draw the one being shown as `find` has it now.
    pub(crate) fn show_here(&self, find: &FindLook, cx: &mut App) {
        let moved: Vec<ElementId> = {
            let mut texts = self.0.borrow_mut();
            let marked = texts.marked.iter_mut();
            marked
                .filter_map(|(id, marked)| {
                    let here = find.nth_here(&marked.text);
                    (std::mem::replace(&mut marked.here, here) != here).then(|| id.clone())
                })
                .collect()
        };
        for id in moved {
            self.draw_marks(&id, cx);
        }
    }

    /// Forgets the matches, once what's found changes, and has each view that drew them draw none.
    pub(crate) fn forget_found(&self, cx: &mut App) {
        let marked = std::mem::take(&mut self.0.borrow_mut().marked);
        for id in marked.keys() {
            if let Some(state) = self.state(id) {
                state.update(cx, TextViewState::clear_range_highlights);
            }
        }
    }

    /// Has the view of `text` bring the match being shown in it into view, now if it has read it, or once it has.
    pub(crate) fn reveal(&self, text: &TextId, cx: &mut App) {
        let id = view_id(text);
        if let Some(marked) = self.0.borrow_mut().marked.get_mut(&id) {
            marked.reveal = Reveal::Wanted;
        }
        self.reveal_if_read(&id, cx);
    }

    /// Whether the view of `text` is waiting to read it before it brings the match being shown into view.
    pub(crate) fn reading_to_reveal(&self, text: &TextId) -> bool {
        self.0.borrow().marked.get(&view_id(text)).is_some_and(|marked| marked.reveal == Reveal::Wanted)
    }

    /// The view of `text`, if it's kept, for a test.
    #[cfg(feature = "testing")]
    pub(crate) fn view_of(&self, text: &TextId) -> Option<Entity<TextViewState>> {
        self.state(&view_id(text))
    }

    /// The view `id` has read more of what it was given, so it draws its matches once it's read all of it, and brings
    /// the one being shown into view if it's to.
    pub(super) fn newly_read(&self, id: &ElementId, cx: &mut App) {
        self.draw_marks(id, cx);
        self.reveal_if_read(id, cx);
    }

    /// The view `id` was given other text, so where its matches are is worked out again once it's read it. The kit
    /// carries its highlights over to the new text until then.
    pub(super) fn text_changed(&self, id: &ElementId) {
        if let Some(marked) = self.0.borrow_mut().marked.get_mut(id) {
            marked.ranges = None;
        }
    }

    fn state(&self, id: &ElementId) -> Option<Entity<TextViewState>> {
        self.0.borrow().kept.get(id).map(|kept| kept.state.clone())
    }

    fn draw_marks(&self, id: &ElementId, cx: &mut App) {
        let Some(state) = self.state(id) else { return };
        let Some(Placed { ranges, here }) = self.placed(id, &state, cx) else { return };
        let highlights: Vec<RangeHighlight> = (ranges.iter().enumerate())
            .map(|(nth, range)| {
                RangeHighlight::new(range.clone(), if here == Some(nth) { look::found_here() } else { look::found() })
            })
            .collect();
        state.update(cx, |state, cx| {
            if highlights.is_empty() {
                state.clear_range_highlights(cx);
            } else {
                let _ = state.set_range_highlights(highlights, cx);
            }
        });
    }

    fn reveal_if_read(&self, id: &ElementId, cx: &mut App) {
        let Some(state) = self.state(id) else { return };
        if !self.0.borrow().marked.get(id).is_some_and(|marked| marked.reveal == Reveal::Wanted) {
            return;
        }
        let Some(Placed { ranges, here }) = self.placed(id, &state, cx) else { return };
        let Some(range) = here.and_then(|nth| ranges.get(nth).cloned()) else { return };
        if let Some(marked) = self.0.borrow_mut().marked.get_mut(id) {
            marked.reveal = Reveal::Asked;
        }
        state.update(cx, |state, cx| {
            let _ = state.reveal_range(range, cx);
        });
    }

    /// Where the needle is in what the view `id` renders, once it's read all of what it was given.
    fn placed(&self, id: &ElementId, state: &Entity<TextViewState>, cx: &App) -> Option<Placed> {
        let mut texts = self.0.borrow_mut();
        let given = texts.kept.get(id)?.text.len();
        let marked = texts.marked.get_mut(id)?;
        if let Some(ranges) = &marked.ranges {
            return Some(Placed { ranges: ranges.clone(), here: marked.here });
        }
        let rendered = state.read(cx).rendered_text();
        if rendered.is_empty() || rendered.source().len() != given {
            return None;
        }
        let ranges: Rc<[Range<usize>]> = markdown::ranges(&marked.needle, rendered.source(), rendered.as_str()).into();
        marked.ranges = Some(ranges.clone());
        Some(Placed { ranges, here: marked.here })
    }
}
