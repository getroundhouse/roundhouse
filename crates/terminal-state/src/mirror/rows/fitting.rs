//! Cells a client wants kept through a new size, such as the top of its view and its selection's ends, which Ghostty
//! keeps a reference to on the same text as it rewraps, as `protocol::terminal::TerminalFitReply` says.

use libghostty_vt::grid_ref::TrackedGridRef;
use libghostty_vt::point::Space;
use protocol::terminal::{Numbering, TerminalFitReply, TerminalPoint};

use crate::{Mirror, MirrorError};

/// References to cells, each kept on its text until this goes, as Ghostty frees a reference when it's dropped.
pub struct Tracked(Vec<Option<TrackedGridRef>>);

impl Mirror {
    /// Keeps a reference to each of `points`, in `numbering`, until what this returns goes. None for one in another
    /// numbering, or past what the terminal keeps.
    pub fn track(&mut self, numbering: Numbering, points: &[TerminalPoint]) -> Result<Tracked, MirrorError> {
        let now = self.numbered()?;
        let mut tracked = Vec::with_capacity(points.len());
        for at in points {
            let at = self.kept(now, numbering, *at)?;
            tracked.push(at.map(|at| self.terminal.track_grid_ref(at)).transpose()?);
        }
        Ok(Tracked(tracked))
    }

    /// Where each cell `tracked` kept is now, in the numbering the terminal is in now, in the same order.
    pub fn tracked(&mut self, tracked: &Tracked) -> Result<TerminalFitReply, MirrorError> {
        let now = self.numbered()?;
        let mut kept = Vec::with_capacity(tracked.0.len());
        for at in &tracked.0 {
            let point = at.as_ref().map(|at| at.point(Space::Screen)).transpose()?.flatten();
            kept.push(point.map(|point| TerminalPoint { row: now.first.plus(u64::from(point.y)), col: point.x }));
        }
        Ok(TerminalFitReply { numbering: now.numbering, kept })
    }
}
