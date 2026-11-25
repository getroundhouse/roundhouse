//! Every request a client sends its hub, spelled out, and every reply the hub sends back, with at least its `result`,
//! since clients and hubs of different builds meet each other.

mod devices;
mod file_diffs;
mod github_apps;
mod pinned;
mod pinned_replies;
mod replies;
mod requests;
mod session_requests;
mod tool_servers;

mod round_trip;
use round_trip::{answered, asked};
