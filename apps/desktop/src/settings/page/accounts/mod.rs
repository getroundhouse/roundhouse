//! Accounts, the page of the logins agents run under, in two lists. Found has each agent's own login on a machine,
//! which the hub learnt of as an agent launched under it, and which Roundhouse only reads. Added has the logins the
//! person gave the hub, each a token the agent's own CLI made, which the hub keeps and supplies according to its
//! selected default for the agent kind. The app never works out which login an agent will run under, since that's the hub's to decide.

use super::Asking;

mod kept;
mod panel;
mod row;
#[cfg(test)]
mod tests;
mod words;

mod account_list;
use account_list::Drawing;
pub(super) use account_list::page;
#[cfg(test)]
use account_list::{listed, older_hub};

mod choosing;
mod defaults;
