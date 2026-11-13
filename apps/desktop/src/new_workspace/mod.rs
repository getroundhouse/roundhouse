//! The new-workspace dialog: which repository, what to work on, the model and its effort, plan mode, and files to go
//! with the first prompt. It asks the app to make the workspace with an event, and the app tells it where one can be
//! made and whether one can be made now, so it never reaches into the app. What's left in it is there the next time it
//! opens, in this app or another, since the hub keeps it, as `keeping` says.

mod keeping;
mod view;

pub(crate) use keeping::Keeping;

mod dialog;

pub(crate) use dialog::{DialogEvent, Offer, open};
pub use dialog::{NewWorkspaceDialog, Request};
