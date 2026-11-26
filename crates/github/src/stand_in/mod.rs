//! A stand-in for GitHub on 127.0.0.1, for the hub's tests and tests of a real hub, serving its API, its device flow
//! and its git at one address. It answers each token's `GET /user`, each ask for a sign-in's code, each ask after a
//! device code's token, each renewal of a sign-in's token, what's asked about an app of your own, each read of pull
//! requests or of issues by number, and each push's first request, as the test says, when the test says, and keeps
//! what it was asked.
#![expect(clippy::unwrap_used, reason = "only tests use it, so failing loudly is what it should do")]

mod answer;
pub use answer::Answer;
pub use waiting::Gate;
mod apps;
mod made_up_key;
mod pull_requests;
pub(crate) use made_up_key::MADE_UP_APP_KEY;
pub use pull_requests::{PullRequestState, PullRequests};
mod references;
pub use references::References;
mod script;
pub use script::Script;
mod serving;
pub use serving::StandIn;
