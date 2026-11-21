//! A hub installed as a service reading the update feed, and telling its clients which release it would update to, as
//! `docs/spec/updates.md` says. The feed is an outside system, so reading it is `reading`'s alone.

mod looking;
mod reading;
#[cfg(test)]
mod tests;

pub(crate) use looking::Updates;
pub use reading::UpdateFeed;
