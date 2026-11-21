//! The transcript view: what it's showing, and how it answers.

pub use events::TranscriptEvent;

mod answers;
mod drawing;
mod events;
mod find;
mod model;
mod outline;
mod proposals;
mod record;
mod sending;
mod shown;
#[cfg(feature = "testing")]
mod testing;

pub(crate) use record::{AtWork, IdleCache};

mod view;
pub(crate) use model::{Did, Model, ProposalCall, SubagentRecords};
pub use view::Transcript;
#[cfg(any(test, feature = "testing"))]
use view::rows_and_loaded;
