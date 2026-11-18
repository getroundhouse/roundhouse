//! Waits through the hub, as a worker asks for them, with a stand-in for GitHub whose pull requests each test sets.

use super::*;
use crate::github::PullRequestState;
use crate::hub::tests::{hub_on_github, hub_with_worktree, taking_worker};
use crate::store::{SessionChange, StatusChange};
use protocol::event::{AgentTask, EventBody, TaskKind};
use protocol::model::SessionStatus;
use protocol::worker::{CancelWait, WorkerAsk, WorkerRequest, WorkerResponse};

mod announced;
mod call_off;

mod waiting;
pub(super) use waiting::wait;
use waiting::{ask, how, start};
