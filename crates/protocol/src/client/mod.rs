//! The platform API: what a desktop app, CLI or phone says to a hub.
//!
//! Each request is a type of its own, which says what the hub answers it with, as [`Request`] does, so a client reads
//! each reply as that, and one of any other kind fails in one place. They're grouped by what they're about, each group
//! a module of its own. On the wire, a request's fields sit beside its `method`, and a reply's beside its `result`.

pub mod accounts;
pub mod admin;
pub mod agent;
pub mod attachments;
pub mod comments;
pub mod drafts;
mod feed;
pub mod files;
mod hello;
pub mod log;
pub mod new_workspace;
pub mod queue;
pub mod records;
mod request;
pub mod sessions;
pub mod subscriptions;
pub mod waits;
pub mod workspaces;

pub use feed::{HubNotice, RecordNotice};
pub use hello::{Hello, HelloReply};
pub use request::ClientRequest;

mod messages;
pub use messages::{
    ApiError, CLIENT_PROTOCOL_VERSION, ClientFrame, ClientResponse, ErrorCode, HubFrame, HubStatus, Outcome, Reply,
    ReplyOutcome, Request, error_chain,
};
