//! A hub installed as a service reading the update feed, telling its clients which release it would update to, and
//! updating to it when one says, as `docs/spec/updates.md` says. The feed is an outside system, so reading it, and
//! downloading from beside it, is `reading`'s alone.

mod looking;
mod placing;
mod reading;
#[cfg(test)]
mod tests;
mod updating;

pub(crate) use looking::Updates;
pub use reading::UpdateFeed;
