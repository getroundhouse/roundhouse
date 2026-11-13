//! Tool calls in the transcript: each is one line, which opens to what the call did and closes again. A read of an
//! image opens to the image. How the wheel scrolls what a call shows, in its block or in the diff an edit shows while
//! it's hovered, is in `wheel.rs`, and an opened command is in `commands.rs`. `subagents.rs` has the calls that start
//! subagents.

#![cfg(test)]

mod commands;
mod wheel;

mod opening;
use opening::open_a_call;
pub(crate) use opening::{COMMAND_LINE, in_a_new_workspace, open_a_command};
