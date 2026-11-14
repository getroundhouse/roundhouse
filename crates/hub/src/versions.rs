//! The versions of each protocol this hub speaks, side by side. Each is only the current one until a break, which
//! brings the one before it for 30 days, as "Changing the protocol" in `docs/architecture.md` says.

use protocol::client::CLIENT_PROTOCOL_VERSION;
use protocol::provider::PROVIDER_PROTOCOL_VERSION;
use protocol::versions::Versions;
use protocol::worker::{WORKER_PROTOCOL_VERSION, previous};

/// What it takes in a client's hello.
pub(crate) const CLIENTS: Versions = Versions::only(CLIENT_PROTOCOL_VERSION);
/// What its hello to a worker names: this protocol, and the one before it, for a worker of an older build.
pub(crate) const WORKERS: Versions = Versions { oldest: previous::VERSION, newest: WORKER_PROTOCOL_VERSION };
/// What its hello to a provider's adapter names.
pub(crate) const PROVIDERS: Versions = Versions::only(PROVIDER_PROTOCOL_VERSION);
