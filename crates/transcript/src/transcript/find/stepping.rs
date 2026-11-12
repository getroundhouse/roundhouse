//! Going from one match to the next: loading its row if it isn't, opening what it's in, and bringing it into view.

use std::rc::Rc;

use gpui_kit::{Context, Window};

use super::count::At;
use super::{bar, is_loaded};
use crate::find::{FindLook, Spot, TextId, reveal};
use crate::rows::found_in_row;
use crate::transcript::Transcript;

/// How many frames the transcript goes on trying to bring a match into view, which only takes more than a couple
/// while its row loads or an area in it scrolls.
const FRAMES_TO_REVEAL: u32 = 120;
/// How many frames Markdown's view says nothing of where the match is before it's taken to be in view, since it says
/// only while it isn't.
const QUIET: u32 = 10;

/// How the match being shown is being brought into view.
#[derive(Default)]
pub(super) struct Revealing {
    /// How many more frames to go on.
    frames: u32,
    /// The Markdown it's in, if it's in Markdown, whose view brings it into view itself.
    markdown: Option<TextId>,
    /// How many frames that view has said nothing for.
    quiet: u32,
}

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
        if self.session.read(|log| is_loaded(log, at.row)) {
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
        if self.session.read(|log| is_loaded(log, at.row)) {
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
            self.session.read(|log| found_in_row(log, &m, &needle, at.row))
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
        let reveal = finding.reveal.clone();
        let find = {
            let mut m = self.model.borrow_mut();
            for (key, fold) in &found.opens {
                m.folds.choose(key.clone(), *fold, true);
            }
            m.find = Rc::new(FindLook::new(Some(needle), Some(spot.clone()), reveal.clone()));
            m.find.clone()
        };
        if found.kit_view {
            self.texts.mark(&found.id, &find, cx);
        }
        self.texts.show_here(&find, cx);
        // What it opened makes its row taller.
        self.list.out_of_date(at.row..at.row + 1);
        if !self.list.shows(at.row) {
            self.list.go_to_row(at.row);
        }
        reveal.want();
        let markdown = found.kit_view.then(|| spot.text.clone());
        if let Some(id) = &markdown {
            self.texts.reveal(id, cx);
        }
        finding.revealing = Revealing { frames: FRAMES_TO_REVEAL, markdown, quiet: 0 };
        cx.notify();
    }

    /// Scrolls the list toward the match being shown, from where it was last drawn, until it's in view below the bar,
    /// a frame at a time. Says what of the list is in view below the bar each frame, for Markdown's view.
    pub(crate) fn keep_revealing(&mut self, cx: &mut Context<Self>) {
        let Some(finding) = &mut self.finding else { return };
        let mut visible = self.list.viewport();
        let under = bar::BOTTOM.min(visible.size.height / 2.);
        visible.origin.y += under;
        visible.size.height -= under;
        let (reveal, revealing) = (finding.reveal.clone(), &mut finding.revealing);
        reveal.set_visible(visible);
        if revealing.frames == 0 || !reveal.is_wanted() {
            return;
        }
        revealing.frames -= 1;
        let Some(drawn) = reveal.drawn() else {
            if let Some(id) = &revealing.markdown
                && !self.texts.reading_to_reveal(id)
            {
                revealing.quiet += 1;
            }
            if revealing.frames == 0 || revealing.quiet > QUIET {
                reveal.done();
            }
            return cx.notify();
        };
        revealing.quiet = 0;
        match reveal::scroll_by(visible, drawn.bounds) {
            Some(by) if revealing.frames > 0 => {
                self.list.scroll_down(by);
                // Markdown's view says where it is again only if it's still out of view.
                reveal.moved();
                cx.notify();
            }
            Some(_) | None => reveal.done(),
        }
    }
}
