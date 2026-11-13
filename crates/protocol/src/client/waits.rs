//! The requests about agents' waits on their pull request, which the hub keeps. A hub older than one refuses it, as
//! `is_unsupported` says.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, Request};
use crate::model::{Session, SessionId, WaitId};

/// Calls off the session's wait `wait`, for the person, as the agent can itself, so it's over as
/// [`WaitEnd::CalledOffByPerson`](crate::model::WaitEnd::CalledOffByPerson) says. It names the wait, as the session's
/// record shows it, so one that has ended calls nothing off, and sent again after the agent has started another, it
/// leaves that one going.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionCallOffWait {
    pub id: SessionId,
    pub wait: WaitId,
}

impl Request for SessionCallOffWait {
    type Reply = Session;

    fn into_request(self) -> ClientRequest {
        ClientRequest::SessionCallOffWait(self)
    }
}
