//! Reading back the files the hub kept with a draft, for a client to put back with what's typed: a chat's from the
//! chat, and the new-workspace dialog's from where the hub keeps them apart, since no chat exists yet.

#[cfg(test)]
use super::FilesFor;
#[cfg(test)]
use crate::{Client, ClientError};

#[cfg(test)]
mod tests;

mod requests;
pub use requests::{CameBack, NotReadBack, ReadBack};
