//! What a call fetches, and how. A server's address, which a person typed, leads to others that the server names, so
//! only `https` is fetched, and only from addresses on the internet. A server that's itself on this machine or a
//! private network is fetched from there, over `http` too, and so is what it names. Each address is checked as it's
//! connected to, since a name can come to any. Each call is within a time limit and reads no more than a limit, and
//! keeps no cookie.

#[cfg(test)]
mod tests;

mod address;
use address::is_special;
mod caller;
pub(crate) use caller::{Answer, Caller, json};
#[cfg(test)]
use caller::{LONGEST_ANSWER, MOST_REDIRECTS};
mod names;
pub(crate) use names::Names;
mod reach;
pub use reach::NotFetched;
pub(crate) use reach::Reach;
mod sending;
pub(crate) use sending::Tls;
