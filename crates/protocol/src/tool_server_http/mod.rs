//! What an MCP server's answer over HTTP says, as everything that sends a server a request on a person's behalf reads it:
//! the hub, as it checks a server and signs in to it, and a worker, as it passes on an agent's calls. One reading for
//! both, so neither sends a server's key anywhere the other wouldn't, or takes a refusal for something the other doesn't.

#[cfg(test)]
mod tests;

mod challenge;
pub use challenge::{Challenge, access_wanted};
mod redirected;
pub use redirected::{MOST_REDIRECTS_WITHIN, Redirected, redirected};
