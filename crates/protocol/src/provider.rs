//! What a hub says to a provider's adapter, over the adapter's stdin and stdout. "Providers" in
//! `docs/architecture.md` has the design: the adapter makes and ends the provider's instances, and the hub
//! reaches each instance's worker itself, by running the command `Connect` answers with.
//!
//! Every request is safe to repeat. `Hello` and `List` get the same answer again, and every other request names
//! its instance and has the same effect however often it's sent. So the hub sends again whatever an adapter that
//! restarted didn't answer.
//!
//! Each request is a type of its own, which says what the adapter answers it with, as [`Request`] does, so a hub reads
//! each reply as that, and one of any other kind fails in one place. On the wire, a request's fields sit beside its
//! `method`, and a reply's beside its `result`.

use serde::{Deserialize, Serialize};

use crate::client::Outcome;
use crate::model::InstanceId;

/// The provider protocol's own version, apart from the client and worker protocols', so a break in one doesn't break
/// the other.
pub const PROVIDER_PROTOCOL_VERSION: u32 = 1;

mod hello;
mod instances;

pub use hello::{Capabilities, Feature, Hello, HelloReply};
pub use instances::{
    Connect, Create, Destroy, Instance, InstanceState, Instances, Kill, List, MakeBaseImage, Size, Start, Stop,
    WorkerBinary, WorkerCommand,
};

/// A request to a provider's adapter, which names what the adapter answers it with.
pub trait Request {
    type Reply: Reply;

    /// It among every other request, as it goes on the wire.
    fn into_request(self) -> ProviderRequest;
}

/// What an adapter answers a request with, read from the response it came as.
pub trait Reply: Sized {
    /// Gives the response back when it's of another kind, for whoever asked to say so.
    fn read(response: ProviderResponse) -> Result<Self, ProviderResponse>;
}

/// What a request that only does something is answered with, once it's done.
impl Reply for () {
    fn read(response: ProviderResponse) -> Result<Self, ProviderResponse> {
        let ProviderResponse::Ok = response else { return Err(response) };
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProviderCall {
    pub id: u64,
    pub request: ProviderRequest,
}

/// Every request. Each says on its type what it does and what it's answered with. One with no fields is a variant with
/// none, rather than one holding its type, since serde would then refuse it with a field a newer hub added, and its
/// type is only for saying what it's answered with.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum ProviderRequest {
    Hello(Hello),
    Create(Create),
    Start(Start),
    Stop(Stop),
    Kill(Kill),
    Destroy(Destroy),
    /// See [`List`].
    List,
    Connect(Connect),
    MakeBaseImage(MakeBaseImage),
    /// A request from a newer hub, which the adapter refuses with `Unsupported`.
    #[serde(other)]
    Unknown,
}

/// Every reply, each read as the type of its own that a request names.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum ProviderResponse {
    Hello(HelloReply),
    Ok,
    Instances(Instances),
    Command(WorkerCommand),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum ProviderFrame {
    Reply {
        id: u64,
        #[serde(flatten)]
        outcome: ProviderOutcome,
    },
    /// Sent unasked.
    Event { event: ProviderEvent },
}

/// An adapter's answer, in the form every protocol's replies take. An adapter refuses with `VersionMismatch` for a hello
/// naming no version it speaks, or a request before hello, `Conflict` for another hub's hello, or a request the
/// instance's state refuses, `NotFound` for an instance that isn't there, `Unavailable` when the provider has no room,
/// such as no spot instances, `Unsupported` for what it doesn't do or can't read, and `Worker` for the rest.
pub type ProviderOutcome = Outcome<ProviderResponse>;

/// What an adapter tells its hub without being asked, so the hub needn't find out when a worker's stream breaks.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderEvent {
    /// It's about to end, such as a spot instance the cloud is taking back, at `at_ms` if the provider says when.
    InstanceEnding {
        instance_id: InstanceId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        at_ms: Option<i64>,
        reason: String,
    },
    /// It's gone, such as with the host it ran on.
    InstanceLost { instance_id: InstanceId, reason: String },
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rest are each the variant that holds them, which the compiler checks.
    #[test]
    fn a_request_with_no_fields_is_sent_as_its_own_method() {
        assert_eq!(serde_json::to_string(&List.into_request()).unwrap(), r#"{"method":"list"}"#);
    }

    /// The rest each read the variant that holds them, which the compiler checks.
    #[test]
    fn a_reply_with_no_fields_reads_as_what_it_says_and_any_other_is_given_back() {
        let instances = ProviderResponse::Instances(Instances { instances: vec![] });

        assert_eq!(<()>::read(ProviderResponse::Ok), Ok(()));
        assert_eq!(<()>::read(instances.clone()), Err(instances.clone()));
        assert_eq!(WorkerCommand::read(instances.clone()), Err(instances));
    }
}
