//! Keeping what's dictated, once the person chooses to: each recording in a folder of its own under `recordings/` in the
//! data directory, as `kept-recordings` has its form. It's this Mac's alone: nothing here goes to a hub or is sent.

mod kept;
mod room;
mod writer;

pub(crate) use kept::observe;
pub(super) use kept::{keep, keeps, start};
#[cfg(any(test, feature = "testing"))]
pub use kept::{keep_recordings, recordings_take};
pub(crate) use room::status_box;
pub use room::{DELETE, STATUS};
