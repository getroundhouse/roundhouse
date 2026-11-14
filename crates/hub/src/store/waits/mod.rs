//! Each agent's waits on its workspace's pull request, from when it asks until they end, and after. Closing a chat or
//! archiving a workspace ends its waits, by the triggers in their migration.

use super::{Store, StoreError, lock, text_of};
#[cfg(test)]
use protocol::model::{Wait, WaitEnd, WaitEnded, WaitId};
#[cfg(test)]
use std::collections::HashSet;

#[cfg(test)]
mod tests;

mod wait_rows;

pub use wait_rows::Seen;

mod remote;
use wait_rows::{COLUMNS, wait_from_row};
