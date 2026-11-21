//! What the tests share: a real hub with the fake agent, which `stack.rs` starts, and the app in a phone-sized window,
//! driven the way someone would. The app pairs and connects as it does on a phone, over TLS, to a hub listening on
//! loopback.

#![cfg(test)]

mod phone;
mod stack;

pub(crate) use phone::{Phone, id};
pub(crate) use stack::Stack;
