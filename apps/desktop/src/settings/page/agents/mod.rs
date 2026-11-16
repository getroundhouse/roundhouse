//! Agents, the page of the logins agents run under, in two lists. Found has each agent's own login on a machine,
//! which the hub learnt of as an agent launched under it, and which Roundhouse only reads. Added has the logins the
//! person gave the hub, each a token the agent's own CLI made, which the hub keeps and supplies according to its
//! selected default for the agent kind. Each is a row in the design of every account page's, as Git's are. The app
//! never works out which login an agent will run under, since that's the hub's to decide.

use super::Asking;

mod account_list;
mod account_menu;
mod card;
mod choosing;
mod defaults;
mod kept;
mod panel;
mod row;
#[cfg(test)]
mod tests;
mod words;

use account_list::Drawing;
pub(super) use account_list::page;
#[cfg(test)]
use account_list::{listed, older_hub};
