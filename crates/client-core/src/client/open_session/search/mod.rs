//! Looking through all of what a view follows for a needle, when the log holds only a window of it: the hub says which
//! of the events hold it, by place, and a search reads each with what's around it, a stretch at a time, for a view to
//! count what the rows it doesn't hold draw, then let go. A chat has one search at the hub at a time, so a query typed a letter at a
//! time doesn't queue a look through the whole chat for each.

#[cfg(test)]
use super::View;
#[cfg(test)]
use crate::session_log::Position;
#[cfg(test)]
use crate::{Client, ClientError};
#[cfg(test)]
use protocol::event::SessionEvent;
#[cfg(test)]
use protocol::find::Needle;
#[cfg(test)]
use std::ops::Range;

#[cfg(test)]
mod tests;

mod searching;
pub(super) use searching::{Asking, Job, Searches};
#[cfg(test)]
use searching::{LONGEST, stretches};
pub use searching::{Search, Searched, Stretch};
