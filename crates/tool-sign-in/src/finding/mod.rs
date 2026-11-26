//! Finding out whether an MCP server takes a sign-in, and where: an MCP message sent as nobody, what its 401 says, the
//! server's metadata, and then its authorization server's, each from where MCP's spec says to look, in its order, and
//! each taken only if it says it's of what was asked.

#[cfg(test)]
mod tests;

mod asking;
mod challenge;
mod found;
pub use found::{Found, NoWay};
mod looking;
#[cfg(test)]
use looking::FIND_WITHIN;
mod metadata;
mod reading;
mod way;
pub use way::{Issuer, Way};
mod well_known;
#[cfg(test)]
use challenge::Challenge;
#[cfg(test)]
use well_known::{of_authorization_server, of_server};
