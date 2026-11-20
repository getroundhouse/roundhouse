//! Telling a worker what the hub has recorded of what it numbered, so it can let go of it. A worker keeps everything
//! numbered until a hub says so, for the next hub, should this one go first. The hub says so as it goes, rather than
//! the next hub saying it as it connects, because each session's record holds only the last number that session
//! recorded, and no one number covers every session. *Unconfirmed.*

#[cfg(test)]
mod tests;

mod telling;

pub(crate) use telling::Acks;
pub(super) use telling::acknowledge;
