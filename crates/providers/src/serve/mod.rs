//! Serves a [`Provider`](crate::Provider) over the provider protocol, on the adapter's stdin and stdout, so an adapter written in Rust
//! is an implementation of the trait and a call to [`serve`].

#[cfg(test)]
use crate::{Provider, ProviderError};
#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
mod tests;

mod requests;
pub use requests::{MakeProvider, serve};
