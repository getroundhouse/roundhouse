//! The issues and pull requests a chat's text links to, which a client asks the hub about for those it shows.

use serde::{Deserialize, Serialize};

use super::{ClientRequest, ClientResponse, Reply, Request};
use crate::model::{ReferenceAsk, ReferenceRead, WorkspaceId};

/// What GitHub says of each of `references`, named in a chat of the workspace `workspace`, read as the workspace's
/// pull request is. The hub keeps what it found for every
/// client, and reads one again only once what it has is a minute old, so a client asks for those it shows, as often
/// as it likes, and two clients showing one chat ask GitHub once. Safe to send again, since it changes nothing but
/// what the hub keeps of GitHub. A hub older than this refuses it as unsupported.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReferencesRead {
    pub workspace: WorkspaceId,
    pub references: Vec<ReferenceAsk>,
}

impl Request for ReferencesRead {
    type Reply = References;

    fn into_request(self) -> ClientRequest {
        ClientRequest::ReferencesRead(self)
    }
}

/// What the hub could say of each reference asked for, in the order they were asked.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct References {
    pub references: Vec<ReferenceRead>,
}

impl Reply for References {
    fn read(response: ClientResponse) -> Result<Self, ClientResponse> {
        let ClientResponse::References(references) = response else { return Err(response) };
        Ok(references)
    }
}
