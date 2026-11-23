//! The versions of each protocol this hub speaks, side by side. Each is only the current one until a break, which
//! brings the one before it until the next break, as `docs/principles/versions.md` says.

use protocol::provider::PROVIDER_PROTOCOL_VERSION;
use protocol::versions::Versions;

/// What it takes in a client's hello.
pub(crate) const CLIENTS: Versions = protocol::client::HUB_SPEAKS;
/// What its hello to a worker names.
pub(crate) const WORKERS: Versions = protocol::worker::HUB_SPEAKS;
/// What its hello to a provider's adapter names.
pub(crate) const PROVIDERS: Versions = Versions::only(PROVIDER_PROTOCOL_VERSION);
