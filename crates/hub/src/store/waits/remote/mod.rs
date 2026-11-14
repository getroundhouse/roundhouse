//! Atomic observation and terminal updates for explicit-target waits.

use super::{COLUMNS, Store, StoreError, lock, text_of, wait_from_row};
use protocol::model::{Wait, WorkspaceId};
use rusqlite::params;
#[cfg(test)]
mod tests;

mod observations;
