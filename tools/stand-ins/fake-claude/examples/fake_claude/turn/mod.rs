//! One turn: running a prompt's script, one command at a time.

mod prose;

use prose::prose;

mod script;
pub(crate) use script::{Block, Turn, begin, message_id, run_turn, state_event};
