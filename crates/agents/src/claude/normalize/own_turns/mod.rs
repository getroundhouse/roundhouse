//! Why the agent starts a turn by itself: the background tasks it heard had ended that no request took in.

#[cfg(test)]
mod tests;

mod tracking;

pub(super) use tracking::OwnTurns;
