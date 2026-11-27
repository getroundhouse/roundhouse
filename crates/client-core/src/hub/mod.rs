//! A hub as a view holds it, for as long as the pairing lasts: its records, what's asked of it, and what's opened from
//! it, kept over every connection the hub's [`Link`](crate::link::Link) makes, so no view is handed a connection to
//! pass on when there's a new one.

mod file_diff;
mod handle;
mod kept;
#[cfg(any(test, feature = "testing"))]
mod on_socket;
#[cfg(test)]
mod tests;

#[cfg(any(test, feature = "testing"))]
pub use handle::LinkHeld;
pub use handle::{Hub, Round, StopIfIdle};
pub(crate) use kept::Kept;
#[cfg(any(test, feature = "testing"))]
pub use on_socket::OnSocket;
