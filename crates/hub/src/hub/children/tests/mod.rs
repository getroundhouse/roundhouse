use crate::store::Move;
use crate::worker_link::WorkerLink;
use parking_lot::Mutex;
use protocol::client::ErrorCode;
use protocol::model::{BranchName, Branches, ChildStatus, Head};
use protocol::worker::{ChildBranch, WorkerRequest, WorktreeRemoval, WorktreeRemoved};
use std::sync::Arc;
use std::time::Duration;

mod gates;
mod removing;
mod settling;

mod creating;
use creating::{BOUND, Seen, StandIn, TRY, at, create, left_being_made, new_branch, remove, taken_so_far, workspace};
