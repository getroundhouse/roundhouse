//! Setting the hub's git credential on a worker's link: the one it has as the link opens, before anything else, and
//! each it has after, in turn, as "The worker's git credential" in `docs/architecture.md` says.

#[cfg(test)]
mod tests;

mod following;
pub(super) use following::Following;
#[cfg(test)]
use following::SET_WITHIN;
