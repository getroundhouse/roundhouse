//! A workspace's terminals in the app, with a real shell: one opened from the `+` after the tabs takes the keys, and
//! its tab goes once it's closed or its shell ends, with the chat back in front.

#![cfg(test)]

mod keys;
mod using;

use using::{rows, showing, stack, type_echo_hi, type_keys, with_a_terminal};
