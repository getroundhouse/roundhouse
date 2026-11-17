//! Connecting a login for agents' accounts to run under, and disconnecting one, as a client asks, choosing a kind's
//! default and moving a chat to an account, and which login an agent is launched under. What's kept, and in what
//! order, is `crate::logins`'s.

#[cfg(test)]
mod tests;

mod login_calls;
