use super::*;
use protocol::event::TaskOutcome;

mod as_recorded;
mod migrated;

mod recording;
use recording::{answer, ended, exited, lately, message, model, other, reading, record, reply, started, subagent};
