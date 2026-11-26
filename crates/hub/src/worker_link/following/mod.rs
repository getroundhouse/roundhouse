//! Setting on a worker's link what the hub has its worker use: the git credential and the hub's tool servers, each as the
//! hub has it as the link opens, before anything else, and each it has after, in turn.
//! `docs/decisions/git-credential-set-by-hub.md` says why the hub sets the credential on the link, and
//! `docs/decisions/hub-holds-tool-servers.md` why a worker has the tool servers.

#[cfg(test)]
mod tests;

mod credential;
mod followed;
mod tool_servers;
pub(super) use credential::Credential;
use followed::Followed;
pub(super) use followed::Following;
#[cfg(test)]
use followed::SET_WITHIN;
pub(super) use tool_servers::Servers;
