//! Workspaces grouped by repository, each with its status, under a heading with the Add project button.

mod drag;
mod fold;
mod footer;
mod heading;
mod hub;
mod kept;
mod nav;
mod project;
mod row;
mod standing;

pub(crate) use kept::Kept;
pub(crate) use row::{doing, lines};
use standing::needs_a_look;
pub(crate) use standing::{Standing, needing_a_look, standing};

mod workspace_list;
#[cfg(test)]
use workspace_list::places;
pub(super) use workspace_list::row_icon;
use workspace_list::{
    GROUP_GAP, HEADING_SIZE, LIST_PAD, Line, LineId, MARK_GAP, MARK_INDENT, ROW_PAD, ROW_SIZE, has_line,
};
pub(crate) use workspace_list::{SORTS, arrange, listed, lists, render, shown};
