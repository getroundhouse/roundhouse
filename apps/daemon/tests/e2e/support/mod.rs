//! What the tests share: a running stack, and ways to wait on and read what it does.

#![cfg(test)]

pub(crate) use feed::next_notice;
pub(crate) use processes::{is_alive, parent_of, signal, terminate};

mod examples;
mod feed;
pub(crate) mod gh;
mod processes;

mod stack;
pub(crate) use stack::{
    Follower, Stack, TIMEOUT, assert_gapless, assistant_text, commit_settings, git, in_mode, install_executable,
    latest_started_pid, pending_approval, read_all, started_pid, wait_for_approval, wait_for_setup, wait_for_status,
    wait_for_turn_end, wait_for_turn_end_while_progressing, wait_until, workspace,
};
