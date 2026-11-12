//! What the sidebar keeps from one frame to the next, besides where it's scrolled to: where each line is on its way,
//! what holds the order still while someone is using the list, and whether the menu of orders is open.

use protocol::model::WorkspaceId;

use super::LineId;
use crate::reorder::{Hold, Places};
use crate::workspace_order::Sort;

#[derive(Default)]
pub(crate) struct Kept {
    /// Where each line is going, and has got to, so one that moves slides there.
    pub(super) places: Places<LineId>,
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
    }
}
