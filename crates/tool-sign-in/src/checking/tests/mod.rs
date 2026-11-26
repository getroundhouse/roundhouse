//! Asking a server for its tools, over real HTTP, as each version of MCP has it, what's made of what it does in place
//! of listing them, and where its key goes. Every server and key is made up.

mod listing;
mod reaching;
mod refusing;
mod stand_in;
use stand_in::{Heard, KEY, Reply, StandIn, as_json, as_stream, held_after, listed, status};
