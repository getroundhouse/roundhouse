//! A new size the window asks for, which keeps the view's top and the selection's ends on their cells, and one from
//! elsewhere, which takes the view to the bottom with nothing selected.

use protocol::terminal::{Numbering, TerminalFitReply, TerminalPoint, TerminalSize};

use super::viewing::{Ask, Window};

/// A fit the window asked for, until its reply and a picture in the numbering it started have both come.
#[derive(Debug)]
pub(super) struct Fitting {
    size: TerminalSize,
    /// Whether the cells it keeps start with the view's top, and go on with the selection's ends, which typing or
    /// turning the wheel meanwhile makes them not.
    top: bool,
    selection: bool,
    /// Its reply's numbering, and where the cells it kept went.
    reply: Option<(Numbering, Vec<Option<TerminalPoint>>)>,
    /// Whether a picture at its size has come before its reply, which the view waits for before it shows it, since
    /// until then it can't say where the view goes.
    pictured: bool,
}

impl Window {
    /// A fit's reply, which puts the view's top and the selection where the cells it kept went, once a picture in
    /// its numbering has come.
    pub(super) fn fitted(&mut self, size: TerminalSize, asked: &[TerminalPoint], reply: &TerminalFitReply) -> Vec<Ask> {
        // One a later fit has taken the place of says nothing of where that one puts the view.
        let Some(fitting) = self.fitting.as_mut().filter(|fitting| fitting.size == size) else {
            return Vec::new();
        };
        let kept: Vec<_> = asked.iter().zip(&reply.kept).map(|(_, kept)| *kept).collect();
        let numbering = reply.numbering;
        let shown = self.rows.screen.as_ref().map(|screen| screen.numbering);
        if shown == Some(numbering) {
            let (top, selection) = (fitting.top, fitting.selection);
            self.fitting = None;
            self.keep(&kept, top, selection);
        } else if fitting.pictured {
            // The picture at its size came in another numbering, as after a new size from elsewhere too.
            self.gone();
        } else {
            fitting.reply = Some((numbering, kept));
            return Vec::new();
        }
        self.changed = true;
        self.fetch()
    }

    /// A picture in a new numbering, at `size`: the one a fit the window asked for started, whose reply says where
    /// the view goes, or one from another device, which takes the view to the bottom with nothing selected, since its
    /// rows have gone.
    pub(super) fn anew(&mut self, numbering: Numbering, size: TerminalSize) {
        let Some(fitting) = self.fitting.as_mut().filter(|fitting| fitting.size == size) else {
            self.gone();
            return;
        };
        match fitting.reply.take() {
            Some((replied, kept)) if replied == numbering => {
                let (top, selection) = (fitting.top, fitting.selection);
                self.fitting = None;
                self.keep(&kept, top, selection);
            }
            Some(_) => self.gone(),
            None => fitting.pictured = true,
        }
    }

    /// Goes to the bottom with nothing selected, and gives up on any fit, as after a new size from elsewhere.
    pub(super) fn gone(&mut self) {
        self.fitting = None;
        self.scrolled = None;
        self.selecting.clear();
    }

    /// Whether it's waiting for a fit's reply to show a picture that has come at its size.
    pub(super) fn waiting(&self) -> bool {
        self.fitting.as_ref().is_some_and(|fitting| fitting.pictured)
    }

    /// Puts the view's top and the selection where a fit's reply says the cells it kept went, as far as they're still
    /// the view's top and the selection's ends.
    fn keep(&mut self, kept: &[Option<TerminalPoint>], top: bool, selection: bool) {
        let mut kept = kept.iter().copied();
        let top = if top { kept.next().flatten() } else { None };
        self.scrolled = top.map(|top| self.clamped(top.row));
        if selection {
            self.selecting.moved(kept.next().flatten(), kept.next().flatten());
        }
    }

    /// The view has room for `room`, which the terminal is asked to take if it isn't that size, or about to be, or the
    /// most a worker gives a terminal, if that's less, which the view shows with the rest of its room empty.
    pub(in crate::client::open_terminal) fn fit(&mut self, room: TerminalSize) -> Vec<Ask> {
        let TerminalSize { cols, rows } = TerminalSize::MOST;
        let room = TerminalSize { cols: room.cols.min(cols), rows: room.rows.min(rows) };
        let changed = self.room != Some(room);
        self.room = Some(room);
        self.fit_to_room(!changed)
    }

    /// Asks for the size the view has room for, unless the terminal is that size or about to be, or `unless_same`.
    pub(super) fn fit_to_room(&mut self, unless_same: bool) -> Vec<Ask> {
        let (Some(room), Some(screen)) = (self.room, &self.rows.screen) else { return Vec::new() };
        let will_be = self.fitting.as_ref().map_or(screen.size, |fitting| fitting.size);
        if unless_same || will_be == room || screen.size == room && self.fitting.is_none() {
            return Vec::new();
        }
        let numbering = screen.numbering;
        let mut keep = Vec::new();
        if let Some(top) = self.scrolled {
            keep.push(TerminalPoint { row: top, col: 0 });
        }
        let range = self.selecting.range();
        if let Some(range) = range {
            keep.extend([range.from, range.to]);
        }
        let (top, selection) = (self.scrolled.is_some(), range.is_some());
        self.fitting = Some(Fitting { size: room, top, selection, reply: None, pictured: false });
        vec![Ask::Fit { size: room, numbering, keep }]
    }

    /// What's done while a fit is on its way: turning the wheel moves the view's top off the cell it kept, selecting
    /// makes the selection another, and typing does both.
    pub(super) fn unkept(&mut self, top: bool, selection: bool) {
        if let Some(fitting) = &mut self.fitting {
            fitting.top &= !top;
            fitting.selection &= !selection;
        }
    }
}
