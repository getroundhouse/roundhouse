//! The order of the sidebar's projects, which dragging a project's row changes. The project's row and its workspaces
//! follow the pointer up and down the list as one, on a raised surface over the others, and the project takes its new
//! place among them as it covers half of each, so they slide out of its way. Held near the top or bottom of the list,
//! it scrolls the list on, so a project can be taken past what's in sight. Let go, it slides the rest of the way to
//! where it has got to, and the order is saved as a setting, so it lasts.

#[cfg(test)]
mod tests;

mod dragging;

pub(crate) use dragging::Dragging;
pub(super) use dragging::{Dragged, groups, ordered, surface, surface_drawn};
#[cfg(test)]
pub(super) use dragging::{merged, slot, stack};
