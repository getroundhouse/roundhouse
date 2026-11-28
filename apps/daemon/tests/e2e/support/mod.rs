//! What the tests share: a running stack, and ways to wait on and read what it does.

#![cfg(test)]

pub(crate) use approvals::{approve_plan, pending_approval, wait_for_approval};
#[cfg(unix)]
pub(crate) use examples::earlier_worker;
pub(crate) use examples::{daemon, misbehaving_worker};
pub(crate) use feed::{next_notice, next_notice_of, session_of, workspace_of};
pub(crate) use gate::Gate;
pub(crate) use observed::{Observing, at_the_hub, branches_once, known};
pub(crate) use processes::is_alive;
#[cfg(unix)]
pub(crate) use processes::{parent_of, signal, terminate};

mod approvals;
pub(crate) mod calls;
mod examples;
mod feed;
mod follower;
mod gate;
pub(crate) mod gh;
pub(crate) mod github;
mod observed;
mod ports;
mod processes;
#[cfg(unix)]
mod raw;
#[cfg(unix)]
pub(crate) use raw::{RawConnection, changes_in, following};

mod stack;
pub(crate) use follower::Follower;
pub(crate) use stack::{
    Stack, TIMEOUT, assert_gapless, assistant_text, commit_settings, git, in_mode, patient_with_workers, read_all,
    session_records, started_pid, wait_for_setup, wait_for_status, wait_for_turn_end, wait_until, workspace,
};
#[cfg(unix)]
pub(crate) use stack::{latest_started_pid, wait_for_turn_end_while_progressing};
