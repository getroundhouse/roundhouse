//! Workspaces grouped by repository, each with its status, under a heading with the Add project button.

mod drag;
mod fold;
mod footer;
mod heading;
mod kept;
mod nav;
mod project;
mod row;
pub(crate) mod signal;
mod standing;

pub(crate) use kept::Kept;
pub(crate) use row::doing;
use standing::needs_a_look;
pub(crate) use standing::{Standing, standing};

mod workspace_list;
#[cfg(test)]
use workspace_list::places;
use workspace_list::{GROUP_GAP, LIST_PAD, Line, LineId, MARK_GAP, REPO_HEIGHT, ROW_HEIGHT, ROW_INSET, has_line};
pub(crate) use workspace_list::{HEADING_HEIGHT, SORTS, arrange, listed, lists, render, shown};
pub(super) use workspace_list::{ICON_BOX, icon_box};
