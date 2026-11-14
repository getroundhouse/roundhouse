//! Connecting a login for agents' accounts to run under, and disconnecting one, as a client asks, and which login an
//! agent is launched under. What's kept, and in what order, is `crate::logins`'s.

#[cfg(test)]
use protocol::client::accounts::AccountConnect;
#[cfg(test)]
use protocol::client::{ErrorCode, HubNotice};

#[cfg(test)]
mod tests;

mod login_calls;
