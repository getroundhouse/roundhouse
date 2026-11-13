//! Files a client sends with a prompt: reading one from disk as the kind its bytes say it is, so every client checks
//! them the same way. `crate::sending` sends them.

#[cfg(test)]
use protocol::attachment::{MAX_FILE_BYTES, MediaType};

#[cfg(test)]
mod tests;

mod sending;
pub use sending::{FileToSend, NotAttached};
