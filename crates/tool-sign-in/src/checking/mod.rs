//! Asking an MCP server for its tools, as a worker's agent would once the person had added it: at the URL they typed,
//! with what it's to know them by. It shows whether the server is there, is an MCP server and takes the key, and
//! nothing it says goes further than that.

#[cfg(test)]
mod tests;

mod answer;
#[cfg(test)]
use answer::LONGEST_ANSWER;
use answer::said;
mod calls;
use calls::Calls;
mod events;
mod listing;
#[cfg(test)]
use listing::CHECK_WITHIN;
mod unreached;
use unreached::unreached;
