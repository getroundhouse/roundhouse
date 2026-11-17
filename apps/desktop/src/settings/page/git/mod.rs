//! Git, the page of the GitHub accounts agents push and open pull requests as, in two lists. Added has the accounts the
//! person gave the hub, by signing in with GitHub or with a token, one of which can be the default, which every
//! machine's git and gh use for github.com. Found has the `gh` login each provider's machines already have, as the hub
//! found it, which agents there use only while the default is each machine's own. The page shows the hub's records and
//! settings as they are: which is the default, how each login stands and how a sign-in goes are the hub's to say.

use super::{Asking, Body, OfHub, Page, PageId, Within};

mod account_menu;
mod added;
mod connect_panels;
mod found;
mod git_page;
mod kept;
mod panel;
mod sign_in;
mod sign_in_panel;
mod sign_in_words;
#[cfg(test)]
mod tests;
mod token;
mod words;

use git_page::Drawing;
pub(super) use git_page::page;
