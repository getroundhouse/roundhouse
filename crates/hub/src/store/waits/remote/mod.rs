//! Atomic observation and terminal updates for explicit-target waits.

use super::{OneValue, Store, StoreError, WaitColumns, lock, text_of, wait_from_row};
use protocol::model::{Wait, WaitId, WaitRead, WorkspaceId};
use rusqlite::{OptionalExtension, params};
use std::collections::{HashMap, HashSet};
#[cfg(test)]
mod tests;

mod observations;
