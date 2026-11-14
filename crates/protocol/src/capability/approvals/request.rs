//! A request for approval, and what it asks.

use serde::{Deserialize, Serialize};

use super::{Capability, Rule};
use crate::capability::asking::{Question, ServerRequest};
use crate::capability::calls::Call;

/// A request for approval, as the agent made it. It takes the place of `ApprovalRequested`'s `tool_name`, `input`,
/// `suggestions`, `asking` and `reason`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub asks: Asks,
    /// Rules the agent suggests keeping, as [`Capability::SuggestedRules`] says.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested: Vec<Rule>,
    /// It comes whatever the mode, as [`Capability::AlwaysAsks`] says.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub always: bool,
    /// Why the agent asks, in its words, such as what its check of a command found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

impl Request {
    /// The capabilities it needs its run to claim.
    pub fn needs(&self) -> Vec<crate::capability::Capability> {
        let Request { asks, suggested, always, why: _ } = self;
        let mut needs = vec![asks.needs()];
        if !suggested.is_empty() {
            needs.push(Capability::SuggestedRules.into());
        }
        if *always {
            needs.push(Capability::AlwaysAsks.into());
        }
        needs
    }
}

/// What a request for approval asks.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "asks", rename_all = "snake_case")]
pub enum Asks {
    /// To make a call, as the call it would make, which is also the call's own, as recorded.
    Call { call: Call },
    /// Questions for the person, in order.
    Questions { questions: Vec<Question> },
    /// To approve a plan, by its words, and the file it's in when the agent wrote it to one.
    Plan {
        plan: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file: Option<String>,
    },
    /// To reach a host on the network, as [`Capability::NetworkRequests`] says.
    Network { host: String, protocol: NetworkProtocol },
    /// For more access than the mode gives, as [`Capability::AccessRequests`] says.
    Access { access: Vec<Access> },
    /// Something a tool server asks the person, through the agent.
    Server(ServerRequest),
    #[serde(other)]
    Unknown,
}

impl Asks {
    /// The capability a request that asks it needs.
    pub fn needs(&self) -> crate::capability::Capability {
        match self {
            Asks::Call { .. } | Asks::Unknown => Capability::Requests.into(),
            Asks::Questions { .. } => crate::capability::asking::Capability::Questions.into(),
            Asks::Plan { .. } => crate::capability::asking::Capability::PlanApproval.into(),
            Asks::Network { .. } => Capability::NetworkRequests.into(),
            Asks::Access { .. } => Capability::AccessRequests.into(),
            Asks::Server(_) => crate::capability::asking::Capability::ServerRequests.into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkProtocol {
    Http,
    Https,
    Socks5Tcp,
    Socks5Udp,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "to", rename_all = "snake_case")]
pub enum Access {
    Read {
        path: String,
    },
    Write {
        path: String,
    },
    Network,
    #[serde(other)]
    Unknown,
}
