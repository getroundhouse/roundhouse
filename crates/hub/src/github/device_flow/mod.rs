//! Signing in with GitHub through its device flow, as an OAuth App: asking GitHub for a code the person types on
//! GitHub, then asking after its token at the pace GitHub sets, until they approve or refuse or the code ends. Each
//! call is within [`ASK_WITHIN`](super::ASK_WITHIN), and every answer of the token's is a 200, told apart by its
//! `error`, as GitHub gave them on Nov 15, 2026.

#[cfg(test)]
use super::stand_in::{Answer, Script, StandIn};
#[cfg(test)]
use super::{ASK_WITHIN, GithubConfig};
#[cfg(test)]
use protocol::client::ErrorCode;

#[cfg(test)]
mod tests;

mod approval;
pub(crate) use approval::{Approval, GithubRefreshToken, Granted};
mod code;
pub(crate) use code::DeviceCode;
#[cfg(test)]
use code::SCOPES;
mod pacing;
#[cfg(test)]
use pacing::{Asked, paced};
mod refusal;
use refusal::refusal;
