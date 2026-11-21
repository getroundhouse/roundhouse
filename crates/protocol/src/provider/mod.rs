//! What a hub says to a provider's adapter, over the adapter's stdin and stdout. The adapter makes and ends the
//! provider's instances, and the hub reaches each instance's worker itself, by running the command `Connect` answers
//! with, as "Workers and providers" in `docs/principles/parts.md` says.
//!
//! Requests are safe to repeat. An instance operation names its instance, and a preflight names its prospective
//! launch. The hub can ask again if the adapter restarts before answering. Requests can overlap, and each reply names
//! the request it answers, within [`HELLO_ANSWERED_WITHIN`] for the hello, [`ANSWERED_AS_THE_HUB_STARTS_WITHIN`] for
//! what a starting hub asks before it serves, and [`ANSWERED_WITHIN`] for the rest.
//!
//! Each request is a type of its own, which says what the adapter answers it with, as [`Request`] does, so a hub reads
//! each reply as that, and one of any other kind fails in one place. On the wire, a request's fields sit beside its
//! `method`, and a reply's beside its `result`.

mod hello;
mod instances;
mod preflight;
pub use preflight::OpenPreflight;

pub use hello::{Capabilities, ClonePersistence, Feature, Hello, HelloReply, WorkerEnvironment, WorkerFilesystem};
pub use instances::{
    Command, Connect, Create, Destroy, EndWorker, Instance, InstanceState, Instances, KillWorker, List, MakeBaseImage,
    Size, Start, WorkerBinary, WorkerCommand,
};

mod messages;
pub use messages::{
    ANSWERED_AS_THE_HUB_STARTS_WITHIN, ANSWERED_WITHIN, HELLO_ANSWERED_WITHIN, PROVIDER_PROTOCOL_VERSION, ProviderCall,
    ProviderEvent, ProviderFrame, ProviderOutcome, ProviderRequest, ProviderResponse, Reply, Request,
};
