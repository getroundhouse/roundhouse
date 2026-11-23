//! The hub's end of the connections workers dial, as `protocol::dialing` says: one iroh endpoint, which admits a
//! worker that holds an unused ticket, or one it admitted before, and holds each admitted worker's connection for
//! whoever waits for its instance's worker.

#[cfg(test)]
mod tests;

mod admitting;
mod arrivals;
mod dial;

pub(crate) use arrivals::Dialed;
pub(crate) use dial::WorkerDial;
