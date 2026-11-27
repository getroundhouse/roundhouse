//! How a worker dials its hub, as `docs/decisions/workers-dial-the-hub.md` says. The hub gives a provider's `Start` a
//! [`WorkerTicket`], which the adapter hands the worker as it is. The worker dials the hub's endpoint, which the ticket
//! names, over iroh, with [`WORKER_ALPN`], opens one stream, and says [`Admission`] on it. The hub answers with an
//! [`Outcome`](crate::client::Outcome) of [`Admitted`], and from then on the stream carries the worker protocol, as the
//! pipes of a worker the hub runs itself do.
//!
//! A ticket admits one worker, once. The hub keeps the key of the worker it admitted, and admits that worker again by
//! its key alone, so one whose connection drops, or whose hub restarts, dials again with the same ticket.

#[cfg(test)]
mod tests;

mod admission;
mod ticket;

pub use admission::{Admission, Admitted, WORKER_ALPN};
pub use ticket::{HubAddress, Ticket, TicketError, TicketSecret, WorkerTicket};
