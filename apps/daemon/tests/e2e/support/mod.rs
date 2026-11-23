//! What the tests share: a running stack, and ways to wait on and read what it does.

#![cfg(test)]

pub(crate) use approvals::{approve_plan, pending_approval, wait_for_approval};
pub(crate) use feed::{next_notice, next_notice_of, session_of, workspace_of};
pub(crate) use gate::Gate;
pub(crate) use processes::{is_alive, parent_of, signal, terminate};

mod approvals;
mod examples;
mod feed;
mod gate;
pub(crate) mod gh;
pub(crate) mod github;
mod processes;
mod raw;
pub(crate) use raw::{RawConnection, changes_in, following};

mod stack;
pub(crate) use stack::{
    Follower, Stack, TIMEOUT, assert_gapless, assistant_text, commit_settings, git, in_mode, install_executable,
    latest_started_pid, patient_with_workers, read_all, session_records, started_pid, wait_for_setup, wait_for_status,
    wait_for_turn_end, wait_for_turn_end_while_progressing, wait_until, workspace,
};
