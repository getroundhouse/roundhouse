//! Copies of the database from before it migrates. A build from before a migration can't open what the
//! migration leaves, so a copy is the way back to it.

#[cfg(test)]
use super::error::BackupError;
#[cfg(test)]
use rusqlite::Connection;
#[cfg(test)]
use std::fs;
#[cfg(test)]
use std::path::Path;

#[cfg(test)]
mod tests;

mod copies;
pub(crate) use copies::back_up;
#[cfg(test)]
pub(crate) use copies::files_in;
#[cfg(test)]
use copies::{KEPT, Name, UNFINISHED};
