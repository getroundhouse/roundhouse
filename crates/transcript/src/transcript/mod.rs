//! The transcript view: what it's showing, and how it answers.

pub use events::TranscriptEvent;

mod answers;
mod events;
mod find;
mod outline;
mod pull_request;
mod record;
mod sending;
#[cfg(feature = "testing")]
mod testing;

pub use pull_request::PullRequestStanding;
pub(crate) use record::{AtWork, IdleCache};

mod view;
pub(crate) use view::Model;
pub use view::Transcript;
#[cfg(any(test, feature = "testing"))]
use view::rows_and_loaded;
