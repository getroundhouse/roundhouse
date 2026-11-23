//! Observations the hub serves: its copies of state someone else owns, such as git, as it last observed them, each
//! saying how fresh it is. `docs/principles/observations.md` has the rules. A new kind of observation is added to
//! [`Observation`] and [`ObservationKey`], and the compiler lists what else has to name it.

#[cfg(test)]
mod tests;

mod account;
mod github;
mod kinds;
mod observed;
mod session;
mod wait;
pub use account::AccountIdentity;
pub use github::{
    IssueSaid, OwnPullRequestDetail, OwnPullRequestSaid, OwnSummary, PullRequestDetail, PullRequestSaid, ReferenceSaid,
};
pub use kinds::{Observation, ObservationKey, ReadFrom};
pub use observed::Observed;
pub use session::ConversationCache;
pub use wait::WaitRead;
