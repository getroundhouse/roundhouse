//! Going from one match to the next: loading its row if it isn't, opening what it's in, and having it brought into
//! view, which what scrolls the chat does, as `crate::reveal` says.

use std::rc::Rc;

use client_core::session_log::Row;
use gpui_kit::{Bounds, Context, Pixels, Window, point, size};

use super::count::At;
use super::{bar, is_loaded};
use crate::find::{FindLook, Spot};
use crate::reveal::Wanted;
use crate::rows::found_in_row;
use crate::texts::Of;
use crate::transcript::Transcript;

impl Transcript {
    /// Shows the next match, or the previous one, going round from the last to the first. With the bar closed, it
    /// opens it. With typing not yet looked for, it looks for it now, and shows the first match.
    pub fn find_next(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.finding.is_none() {
            return self.open_find(window, cx);
        }
        if self.typing(cx) {
            return self.find_again(cx);
        }
        let Some(query) = self.query_mut() else { return };
        // Going anywhere is going there instead of to the first.
        query.first = false;
        let next = if forward { query.counts.next(query.at) } else { query.counts.previous(query.at) };
        if let Some(at) = next {
            self.select(at, cx);
        }
    }

    /// Makes `at` the match shown, once its row is loaded.
    pub(super) fn select(&mut self, at: At, cx: &mut Context<Self>) {
        let Some(query) = self.query_mut() else { return };
        query.at = Some(at);
        query.going_to = Some(at);
        if is_loaded(&self.log, at.row) {
            self.show(at, cx);
        } else {
            // Drawing its row asks for its page, and the log says once it's here.
            self.list.go_to_row(at.row);
        }
        cx.notify();
    }

    /// Shows the match being gone to, if its row has loaded.
    pub(crate) fn go_on(&mut self, cx: &mut Context<Self>) {
        let Some(at) = self.query().and_then(|query| query.going_to) else { return };
        if is_loaded(&self.log, at.row) {
            self.show(at, cx);
        }
    }

    /// Shows `at`, in a row that's loaded: opens what it's in, has it drawn as the match being shown, and brings it into
    /// view.
    fn show(&mut self, at: At, cx: &mut Context<Self>) {
        let Some(finding) = &mut self.finding else { return };
        let Some(query) = &mut finding.query else { return };
        query.going_to = None;
        let needle = query.needle.clone();
        let texts = {
            let m = self.model.borrow();
            found_in_row(&self.log, &m, &needle, at.row)
        };
        let spots: Vec<(&crate::rows::Found, usize)> =
            texts.iter().flat_map(|found| (0..found.matches).map(move |nth| (found, nth))).collect();
        // The row draws a different number of matches now that it's loaded, such as when it was counted from a stretch
        // that didn't have a subagent's work in it.
        if query.counts.place(at).is_none() || spots.len() != query.counts.of(at.row) {
            query.counts.set(at.row, spots.len());
            match query.counts.kept(at) {
                Some(kept) if kept != at => return self.select(kept, cx),
                Some(_) => {}
                None => {
                    query.at = None;
                    return cx.notify();
                }
            }
        }
        let Some((found, nth)) = spots.get(at.nth).copied() else { return };
        let spot = Spot { text: found.id.clone(), nth };
        let reveal = self.selection.reveal().clone();
        {
            let mut m = self.model.borrow_mut();
            for (key, fold) in &found.opens {
                m.folds.choose(key.clone(), *fold, true);
            }
            m.find = Rc::new(FindLook::new(Some(needle), Some(spot), reveal.clone(), look::colors(cx)));
        }
        // What it opened makes its row taller.
        self.list.out_of_date(at.row..at.row + 1);
        // What scrolls the chat goes to its row first, if it doesn't show, as the next frame starts.
        if let Row::Event(event) = self.log.row(at.row) {
            reveal.want(Wanted::Match, Of::Event(event.seq));
        }
        cx.notify();
    }
}

/// What of `visible`, where the chat shows its rows, is in view: what's below Find's bar while it's `open`, since the
/// bar hides the chat's top, but at most half of it, and all of it otherwise.
pub(in crate::transcript) fn in_view(open: bool, visible: Bounds<Pixels>) -> Bounds<Pixels> {
    if !open {
        return visible;
    }
    let under = bar::bottom().min(visible.size.height / 2.);
    Bounds::new(
        point(visible.origin.x, visible.origin.y + under),
        size(visible.size.width, visible.size.height - under),
    )
}
