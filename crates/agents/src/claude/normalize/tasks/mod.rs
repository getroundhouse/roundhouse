//! The agent's background tasks: the CLI's list of them, which of them are monitors, the call that started each, and
//! which left the list stopped rather than finishing.

#[cfg(test)]
mod tests;

mod tracking;

pub(super) use tracking::{Tasks, listed};
