//! Every request a client sends its hub, spelled out, and every reply the hub sends back, with at least its `result`,
//! since clients and hubs of different builds meet each other.

mod devices;
mod github_apps;
mod pinned;
mod replies;
mod requests;
mod session_requests;

mod round_trip;
use round_trip::{answered, asked};
