//! A stand-in for an MCP server and its authorization server, at one address on 127.0.0.1, for this crate's tests and
//! the hub's. It's a sign-in's whole other side, the person's browser too: it checks what it's asked as an authorization
//! server does, the verifier and the server the tokens are for among it, gives tokens its own MCP server then takes,
//! answers as the test says, which the test can change as it goes, and keeps what it was asked.
#![expect(clippy::unwrap_used, reason = "only tests use it, so failing loudly is what it should do")]

pub use waiting::Gate;
mod asked;
pub use asked::Asked;
mod authorization;
mod metadata;
mod replies;
mod script;
pub use script::{Asks, Iss, MetadataAt, Person, Refreshing, Says, Script, ServerMetadataAt};
mod serving;
pub use serving::StandIn;
mod tokens;
