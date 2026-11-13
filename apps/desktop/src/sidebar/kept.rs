//! What the sidebar keeps from one frame to the next, besides where it's scrolled to: where each line is on its way,
//! which projects are folding, where each project is and which is being dragged, what holds the order still while
//! someone is using the list, and whether the menu of orders is open.

use std::collections::HashSet;
use std::ops::Range;

use gpui_kit::Pixels;
use protocol::model::{RepoId, WorkspaceId};

use super::LineId;
use super::drag::Dragging;
use crate::reorder::{Hold, Places};
use crate::workspace_order::Sort;

#[derive(Default)]
pub(crate) struct Kept {
    /// Where each line is going, and has got to, so one that moves slides there.
    pub(super) places: Places<LineId>,
    /// The projects whose workspaces are on their way out from under their rows, or away under them, from the press
    /// that sent them until what comes after and the project's row have both got to their places. Which projects are
    /// collapsed is a setting.
    pub(super) folding: HashSet<RepoId>,
    /// How far down the list each project's lines reach, at rest, in the order listed, as of the last frame, which is
    /// what a dragged project's place is worked out from.
    pub(super) groups: Vec<(RepoId, Range<Pixels>)>,
    /// The project being dragged, whose lines follow the pointer, and the order it has put the projects in, until it's
    /// let go and back in its place.
    pub(super) dragging: Option<Dragging>,
    /// What holds the order still while someone is using the list, for as long as the order chosen stays the same.
    pub(super) hold: Hold<Sort>,
    /// The menu of orders is open, so the button it's from shows wherever the pointer is.
    pub(super) sort_menu_open: bool,
    /// The workspace whose row is held pressed. GPUI has a pressed row say the pointer has left it at the first move,
    /// and a quick press moves, so until the press is let go the pointer is taken to be where it was.
    pub(super) pressed: Option<WorkspaceId>,
}

impl Kept {
    /// The list isn't drawn this frame, such as while Settings shows, so it hears nothing of the pointer or of the
    /// menu of orders, and neither can be taken to be on it once it's drawn again.
    pub(crate) fn not_drawn(&mut self) {
        self.hold.not_drawn();
        self.sort_menu_open = false;
        self.pressed = None;
        if let Some(dragging) = &mut self.dragging {
            dragging.not_drawn();
        }
    }
}
