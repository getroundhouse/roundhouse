//! A workspace's terminals in the app: one opened from the `+` after the tabs takes the keys, and its tab goes once
//! it's closed or its shell ends, with the chat back in front, with a real shell, and what's selected in one is what's
//! copied, on a fake hub's terminal.

#![cfg(test)]

mod keys;
mod selecting;
mod using;

use using::{rows, showing, stack, type_keys, type_line, type_text, wait_for_a_row, with_a_terminal};
