//! A stand-in for `gh`, since GitHub can't be reached from a test.

use super::{Stack, install_executable};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
mod remote;

mod stand_in;
pub(crate) use stand_in::LOGGED_OUT;
pub(crate) use stand_in::MERGED;
pub(crate) use stand_in::MOVED_ON;
pub(crate) use stand_in::NEWER;
pub(crate) use stand_in::NONE;
pub(crate) use stand_in::OFFLINE;
pub(crate) use stand_in::OPEN;
pub(crate) use stand_in::OPEN_URL;
pub(crate) use stand_in::SOMEONE_ELSES;
pub(crate) use stand_in::StandInGh;
