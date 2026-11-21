//! What tests read of finding: where the match being shown was drawn, and what each text has, drawn and counted.

use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::{Context, Window};
use protocol::find::Needle;

use super::is_loaded;
use crate::find::FindLook;
use crate::reveal::Reveal;
use crate::rows::{Found, found_in_row};
use crate::transcript::Transcript;

impl Transcript {
    /// Whether the match being shown was last drawn in view below the bar.
    pub fn match_in_view(&self) -> Option<bool> {
        self.finding.as_ref()?;
        self.selection.reveal().shows_in_view()
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
        let rows: Vec<usize> = (0..self.log.row_count()).filter(|ix| is_loaded(&self.log, *ix)).collect();
        let mut counted = BTreeMap::new();
        {
            let mut m = self.model.borrow_mut();
            let found: Vec<Found> = rows.iter().flat_map(|row| found_in_row(&self.log, &m, &needle, *row)).collect();
            for found in found {
                for (key, fold) in &found.opens {
                    m.folds.choose(key.clone(), *fold, true);
                }
                counted.insert(format!("{:?}", found.id), found.matches);
            }
            m.find = Rc::new(FindLook::new(Some(needle), None, Reveal::default(), look::colors(cx)));
        }
        let row = self.frame_rows(look::LineText::new(window, cx), cx);
        for ix in rows {
            row(ix, cx);
        }
        let drawn = self.model.borrow().find.drawn();
        let drawn =
            drawn.into_iter().filter(|(_, matches)| *matches > 0).map(|(id, matches)| (format!("{id:?}"), matches));
        (drawn.collect(), counted)
    }
}
