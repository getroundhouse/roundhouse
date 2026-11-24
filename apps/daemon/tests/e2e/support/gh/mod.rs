//! A stand-in for `gh`, since GitHub can't be reached from a test.

use super::Stack;
use std::path::PathBuf;

mod stand_in;
pub(crate) use stand_in::LOGGED_IN_AS;
pub(crate) use stand_in::LOGGED_IN_WITH;
pub(crate) use stand_in::StandInGh;
