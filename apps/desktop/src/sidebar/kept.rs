//! What the sidebar keeps from one frame to the next, besides where it's scrolled to: where each line is on its way,
//! what holds the order still while someone is using the list, whether the menu of orders is open, and which
//! workspaces there were before one was made.

use std::collections::HashSet;

use protocol::model::{Workspace, WorkspaceId};

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
    /// Every workspace there was as the one being made was asked for, so the one that's new is the one not here.
    before_making: Option<HashSet<WorkspaceId>>,
}

impl Kept {
    /// The list isn't drawn this frame, such as while Settings shows, so it hears nothing of the pointer or of the
    /// menu of orders, and neither can be taken to be on it once it's drawn again.
    pub(crate) fn not_drawn(&mut self) {
        self.hold.not_drawn();
        self.sort_menu_open = false;
    }

    /// A workspace is about to be asked for, among `workspaces`.
    pub(crate) fn making_among(&mut self, workspaces: &[Workspace]) {
        self.before_making = Some(workspaces.iter().map(|w| w.id.clone()).collect());
    }

    /// Whether the workspace being made is among `listed`, places in `workspaces`, so it takes the note's place.
    pub(super) fn made_is_listed(&self, workspaces: &[Workspace], listed: &[usize]) -> bool {
        let Some(before) = &self.before_making else { return false };
        listed.iter().any(|&w| !before.contains(&workspaces[w].id))
    }
}

#[cfg(test)]
mod tests;
