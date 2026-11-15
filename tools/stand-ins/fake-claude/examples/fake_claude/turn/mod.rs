//! One turn: running a prompt's script, one command at a time.

mod prose;

use prose::prose;

mod script;
mod writing;
pub(crate) use script::{begin, run_turn, state_event};
pub(crate) use writing::{Block, Turn, Within, message_id};
