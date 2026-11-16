//! A hub that answers from a fixed [`World`], so a screen looks the same on every run. It speaks the real protocol
//! over a real socket, so the app is unchanged, and it refuses anything that would change what it holds.

#![cfg(test)]

mod serve;
mod stand_in;

pub(crate) use serve::serve;
pub(crate) use stand_in::{BUILD, BUILD_NUMBER};
