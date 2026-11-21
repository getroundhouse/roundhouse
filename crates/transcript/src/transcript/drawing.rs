//! How the transcript draws itself each frame: its list's rows, from the version of the log it last caught up with,
//! with what's around them, such as Find's bar and the outline.

use client_core::session_log::SessionLog;
use gpui_kit::{AnyElement, App, Context, ElementId, IntoElement, ParentElement as _, Render, Window};
use look::LineText;

use super::{Transcript, find};
use crate::list::Rows;
use crate::step_keys::with_step_keys;
use crate::texts::Of;

impl Transcript {
    /// What draws each of the list's rows in this frame, by its index, from the version of the log the transcript last
    /// caught up with, as `rows`' `render_row` says.
    pub(super) fn frame_rows(
        &self,
        line_text: LineText,
        cx: &mut Context<Self>,
    ) -> impl Fn(usize, &mut App) -> Option<AnyElement> + 'static {
        let (model, log) = (self.model.clone(), self.log.clone());
        let shared = crate::rows::Shared {
            this: cx.weak_entity(),
            linked: self.linked.clone(),
            references: self.references.clone(),
            line_text,
            texts: self.texts.clone(),
            selection: self.selection.clone(),
            moving: self.moving.clone(),
        };
        let tail = self.list.row_count().saturating_sub(1);
        move |ix, cx| crate::rows::render_row(&model, &log, &shared, crate::rows::Item { ix, tail }, cx)
    }
}

impl Render for Transcript {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Its rows leave room for the marks that move again as they're drawn.
        self.moving.draw_again();
        self.references.drawing(cx);
        let wanted = self.session.clone();
        let recorded = self.log.row_count();
        let line_text = LineText::new(window, cx);
        let row = self.frame_rows(line_text.clone(), cx);
        let room_under = crate::rows::room_under(&self.model.borrow());
        let (log, rows, finding) = (self.log.clone(), self.list.row_count(), self.finding.is_some());
        let rows = Rows {
            draw: move |ix, _: &mut Window, cx: &mut App| row(ix, cx),
            want: move |rows: std::ops::Range<usize>, unfetched: Option<usize>| {
                // This frame's tail isn't a log row. Before the first catch-up it is row 0, even if history has arrived
                // on the session's thread; asking for it there would fetch the start of a chat opened at its end.
                if recorded > 0 {
                    wanted.want(rows.start..rows.end.min(recorded), unfetched.filter(|row| *row < recorded));
                }
            },
            row_of: move |of| row_of(&log, rows, of),
        };
        let id = ElementId::Name(self.list_id.clone().into());
        let in_view = move |visible| find::in_view(finding, visible);
        let rows = self.list.render(id, rows, in_view, room_under, window, cx);
        let bar = self.find_bar(cx);
        let chat = self.with_outline(rows.into_any_element(), line_text, window, cx).children(bar);
        with_step_keys(self.selection.with_keys(chat), cx)
    }
}

/// The row of the list's `rows` that draws what `of` says, by the version of the log `log`, if one does.
pub(super) fn row_of(log: &SessionLog, rows: usize, of: Of) -> Option<usize> {
    match of {
        Of::Event(seq) => log.row_of(seq),
        Of::Head | Of::Tail | Of::Foot | Of::Document(_) => {
            rows.checked_sub(1).and_then(|tail| crate::rows::list_row_of(of, tail))
        }
    }
}
