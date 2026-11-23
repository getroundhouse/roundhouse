//! Observations the hub serves: its copies of state someone else owns, such as git, as it last observed them, each
//! saying how fresh it is. `docs/principles/observations.md` has the rules. A new kind of observation is added to
//! [`Observation`] and [`ObservationKey`], and the compiler lists what else has to name it.

#[cfg(test)]
mod tests;

mod kinds;
mod observed;
pub use kinds::{Observation, ObservationKey};
pub use observed::Observed;
