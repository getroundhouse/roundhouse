//! A list of workspaces whose order can change while someone is using it, as the sidebar's and Home's can. Each row
//! that moves slides from where it was to its new place, rather than jumping, so it's plain where it went, and the
//! order holds still while someone is using the list, so a row doesn't move out from under a click. A list keeps a
//! [`Places`] and a [`Hold`] of its own from one frame to the next, and draws its rows with [`list`], each where it
//! has got to.

mod hold;
mod list;
mod places;

#[cfg(feature = "testing")]
pub(crate) use hold::HOLDS_FOR;
pub(crate) use hold::{Hold, hears_the_pointer, hold_while_open, keep_held};
pub(crate) use list::{Drawn, list};
pub(crate) use places::{Arrangement, Places};
