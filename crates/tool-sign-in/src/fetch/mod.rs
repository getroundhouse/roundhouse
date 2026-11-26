//! What a call fetches, and how. A server's address, which a person typed, leads to others that the server names, so
//! only `https` is fetched, and only from addresses on the internet. A server that's itself on this machine or a
//! private network is fetched from there, over `http` too, and so is what it names. Each address is checked as it's
//! connected to, since a name can come to any. Which addresses are private is `tool_server_http`'s rule, which a
//! worker goes by too as it passes an agent's call on with a sign-in. Each call is within a time limit and reads no more than a
//! limit, and keeps no cookie. A server is asked for its tools by `crate::checking`, over connections made here, with
//! no such rules, since it's asked only at the address a person typed.

#[cfg(test)]
mod tests;

mod caller;
pub(crate) use caller::{Answer, Caller, json};
#[cfg(test)]
use caller::{LONGEST_ANSWER, MOST_REDIRECTS};
mod reach;
pub use reach::NotFetched;
pub(crate) use reach::{Reach, url_of};
