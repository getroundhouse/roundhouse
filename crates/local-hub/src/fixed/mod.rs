//! Where a Windows hub runs from: [`Paths::fixed_hub`], one path for every build, so the leave to listen that the
//! firewall gives a program by its path outlasts an update. It's a second name for the build's copy, so what marks the
//! copy as run from marks it too. Only Windows uses it, since a Mac's firewall goes by a program's signature and
//! Linux's by its port, but it's only files, so its tests run everywhere.
//!
//! [`Paths::fixed_hub`]: protocol::paths::Paths::fixed_hub

#[cfg(test)]
mod tests;

mod placing;

pub(super) use placing::{place, remove_leftovers};
