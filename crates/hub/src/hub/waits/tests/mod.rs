//! Waits through the hub, as a worker asks for them, with a stand-in for the workspace's worker that reads its pull
//! request as each test says.

use super::*;
pub(super) use crate::hub::tests::open;
use crate::hub::tests::{hub_with_worktree, reading, says};
use crate::store::{Seen, SessionChange, StatusChange};
use protocol::event::{AgentTask, EventBody, TaskKind};
use protocol::model::{PrState, PullRequest, SessionStatus};
use protocol::worker::{CancelWait, WorkerAsk, WorkerRequest, WorkerResponse};

mod announced;
mod call_off;

mod waiting;
pub(super) use waiting::wait;
use waiting::{ask, how, start};
