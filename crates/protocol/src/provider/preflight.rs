//! A route to where an agent in a project would run, before any workspace there does.

use super::{Command, ProviderRequest, Request};
use crate::model::{AgentKind, RepoOrigin};
use serde::{Deserialize, Serialize};

/// Returns the command whose stdin and stdout carry the exchange in [`crate::preflight`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct OpenPreflight {
    pub project: RepoOrigin,
    pub agent: AgentKind,
}

impl Request for OpenPreflight {
    type Reply = Command;

    fn into_request(self) -> ProviderRequest {
        ProviderRequest::OpenPreflight(self)
    }
}
