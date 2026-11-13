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

mod hello;
mod instances;

pub use hello::{Capabilities, Feature, Hello, HelloReply};
pub use instances::{
    Connect, Create, Destroy, Instance, InstanceState, Instances, Kill, List, MakeBaseImage, Size, Start, Stop,
    WorkerBinary, WorkerCommand,
};

mod messages;
pub use messages::{
    PROVIDER_PROTOCOL_VERSION, ProviderCall, ProviderEvent, ProviderFrame, ProviderOutcome, ProviderRequest,
    ProviderResponse, Reply, Request,
};
