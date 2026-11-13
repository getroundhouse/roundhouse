use super::super::Line;
use crate::reorder::Arrangement;
use gpui_kit::{Pixels, px};

/// How a project's workspaces stand in this frame. Each line has its project's, which for a line above the first
/// project is `Open`.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(in super::super) enum Fold {
    /// They show under its row.
    #[default]
    Open,
    /// They're on their way out from under its row.
    Opening,
    /// The project is collapsed, and they're on their way under its row.
    Closing,
    /// The project is collapsed, and they're under its row, where they aren't drawn.
    Collapsed,
}

impl Fold {
    /// A project's, from whether it's collapsed and whether its workspaces are still on their way.
    pub(in super::super) fn of(collapsed: bool, on_their_way: bool) -> Self {
        match (collapsed, on_their_way) {
            (false, false) => Fold::Open,
            (false, true) => Fold::Opening,
            (true, true) => Fold::Closing,
            (true, false) => Fold::Collapsed,
        }
    }

    pub(in super::super) fn collapsed(self) -> bool {
        matches!(self, Fold::Closing | Fold::Collapsed)
    }

    /// Whether its workspaces are on their way out from under its row, or away under it.
    pub(in super::super) fn moving(self) -> bool {
        matches!(self, Fold::Opening | Fold::Closing)
    }

    /// Whether its workspaces are lines in this frame, which they still are on their way under its row.
    pub(in super::super) fn has_lines(self) -> bool {
        self != Fold::Collapsed
    }
}

/// Each line's project's [`Fold`], which `of` says for the project a `Line::Repo` names.
pub(in super::super) fn folds(lines: &[Line], of: impl Fn(usize) -> Fold) -> Vec<Fold> {
    let mut fold = Fold::default();
    let own = |line: &Line| {
        match *line {
            Line::Heading => fold = Fold::default(),
            Line::Repo(repo) => fold = of(repo),
            Line::Workspace(_) | Line::Creating(_) => {}
        }
        fold
    };
    lines.iter().map(own).collect()
}

/// Whether a line takes room once everything has come to rest, which a collapsed project's workspace doesn't.
pub(in super::super) fn rests(line: Line, fold: Fold) -> bool {
    !line.is_workspace() || !fold.collapsed()
}

/// A line as it's drawn in this frame.
#[derive(Clone, Copy)]
pub(in super::super) struct Drawn {
    pub(in super::super) line: Line,
    /// Its project's fold.
    pub(in super::super) fold: Fold,
    /// How far down the list its top is.
    pub(in super::super) top: Pixels,
    /// Whether it's still on its way to where it's drawn at rest, or, for a workspace's row while its project folds,
    /// whether the project's row is.
    pub(in super::super) moving: bool,
    /// How much of a workspace's row shows, from its top, while its project folds. `None` for a line that isn't cut
    /// off.
    pub(in super::super) cut: Option<Pixels>,
    /// For a project's row, how far out from under it its workspaces are, from 0 to 1, which its chevron turns by.
    pub(in super::super) out: f32,
}

/// Where each line is in this frame.
pub(in super::super) struct Laid {
    pub(in super::super) lines: Vec<Drawn>,
    /// How far down the list ends.
    pub(in super::super) end: Pixels,
    /// The projects, as `Line::Repo` names them, whose fold has ended.
    pub(in super::super) settled: Vec<usize>,
}

/// The workspaces' rows of a project that's folding, from its row down.
struct Folding {
    /// Where its next row is drawn.
    pub(super) next: Pixels,
    /// The top edge of what comes after it, less the space kept between them, which its rows are cut off at.
    pub(super) edge: Pixels,
    /// Whether the project's row is still on its way to its place.
    pub(super) moving: bool,
}

/// Says where each of `lines` is drawn, each with its project's fold. `sent` has each line that [`rests`], in order,
/// and after them where the list ends, as [`Places`](crate::reorder::Places) sent them. A folding project's
/// workspaces go by its row instead, since those on their way under it have no place to go to.
pub(in super::super) fn lay(lines: &[Line], folds: &[Fold], sent: &Arrangement) -> Laid {
    let at = |resting: usize| sent.drawn[resting];
    let end = sent.drawn.len() - 1;
    let gap_below = |from: usize| lines.get(from + 1).map_or(px(0.), |next| lines[from].gap_before(*next));
    let (mut drawn, mut settled) = (Vec::new(), Vec::new());
    // How many lines that rest came before this one.
    let (mut resting, mut folding) = (0, None);
    for (index, (&line, &fold)) in lines.iter().zip(folds).enumerate() {
        let (top, cut, moving) = match &mut folding {
            Some(Folding { next, edge, moving }) if line.is_workspace() => {
                let top = *next;
                *next += line.height() + gap_below(index);
                (top, Some((*edge - top).clamp(px(0.), line.height())), *moving)
            }
            _ => (at(resting), None, sent.moving[resting]),
        };
        let mut out = 1.;
        if let Line::Repo(repo) = line {
            let rows = lines[index + 1..].iter().take_while(|line| line.is_workspace()).count();
            let (after, last) = (resting + 1 + if fold.collapsed() { 0 } else { rows }, index + rows);
            // The space kept above what comes after, and where the rows start. Every row with the space between
            // them is what the fold covers.
            let (kept, under) = (gap_below(last), top + line.height() + gap_below(index));
            let all = (index + 1..=last).map(|row| lines[row].height() + gap_below(row)).sum::<Pixels>() - kept;
            let edge = at(after) - kept;
            out = match fold {
                Fold::Opening | Fold::Closing if all > px(0.) => ((edge - under) / all).clamp(0., 1.),
                Fold::Open | Fold::Opening => 1.,
                Fold::Closing | Fold::Collapsed => 0.,
            };
            folding = fold.moving().then_some(Folding { next: under, edge, moving });
            // Both, since what comes after can be on a clock of its own, as when a project above it was folded a
            // moment before, and the rows, which go by the project's row, would jump were they let go while it moved.
            if fold.moving() && !sent.moving[after] && !moving {
                settled.push(repo);
            }
        }
        if rests(line, fold) {
            resting += 1;
        }
        drawn.push(Drawn { line, fold, top, moving, cut, out });
    }
    Laid { lines: drawn, end: at(end), settled }
}
