//! What tests read of finding: where the match being shown was drawn, and what each text has, drawn and counted.

use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::{Context, Window};
use protocol::find::Needle;

use super::is_loaded;
use crate::find::FindLook;
use crate::find::reveal::Reveal;
use crate::rows::{Found, Item, Shared, found_in_row, render_row};
use crate::transcript::Transcript;

impl Transcript {
    /// Whether the match being shown was last drawn in view below the bar.
    pub fn match_in_view(&self) -> Option<bool> {
        self.finding.as_ref()?.reveal.shows_in_view()
    }

    /// How many matches of `query` each text of the rows the log holds has, `(drawn, counted)`: as the rows draw them,
    /// with everything open that has any, and as counting finds them, for a test that the two agree. A closed call's
    /// summary isn't drawn, since everything's open.
    pub fn found_both_ways(
        &mut self,
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (BTreeMap<String, usize>, BTreeMap<String, usize>) {
        let Ok(needle) = Needle::new(query) else { return Default::default() };
        let rows: Vec<usize> = self.session.read(|log| (0..log.row_count()).filter(|ix| is_loaded(log, *ix)).collect());
        let mut counted = BTreeMap::new();
        {
            let mut m = self.model.borrow_mut();
            let found: Vec<Found> =
                self.session.read(|log| rows.iter().flat_map(|row| found_in_row(log, &m, &needle, *row)).collect());
            for found in found {
                for (key, fold) in &found.opens {
                    m.folds.choose(key.clone(), *fold, true);
                }
                counted.insert(format!("{:?}", found.id), found.matches);
            }
            m.find = Rc::new(FindLook::new(Some(needle), None, Reveal::default()));
        }
        let line_text = look::LineText::new(window, cx);
        let shared = Shared {
            this: cx.weak_entity(),
            linked: self.linked.clone(),
            line_text,
            prose: self.prose.clone(),
            selection: self.selection.clone(),
        };
        let tail = self.list.row_count().saturating_sub(1);
        for ix in rows {
            render_row(&self.model, &self.session, &shared, Item { ix, tail }, cx);
        }
        let drawn = self.model.borrow().find.drawn();
        let drawn =
            drawn.into_iter().filter(|(_, matches)| *matches > 0).map(|(id, matches)| (format!("{id:?}"), matches));
        (drawn.collect(), counted)
    }
}
