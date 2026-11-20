//! Git, the page of the GitHub accounts agents push and open pull requests as, in two lists. Added has the accounts the
//! person gave the hub, by signing in with GitHub, with a token, or as an app of your own's installation, one of which
//! can be the default, which git and gh use for github.com on every provider where Roundhouse sets up GitHub, and each
//! app installed nowhere yet. Found has the `gh` login each provider's machines already have, as the hub found it,
//! which agents on the hub's own machine always use, and elsewhere only while the default is each machine's own. The page shows the hub's records and settings as they are: which is the
//! default, how each login stands, how a sign-in goes and where an app's installed are the hub's to say.

use super::{Asking, Body, OfHub, Page, PageId, Within};

mod account_menu;
mod added;
mod app_calls;
mod app_made;
mod app_menu;
mod app_panel;
mod app_panel_drawn;
mod app_rows;
mod app_words;
mod connect_panels;
mod found;
mod gh_choice;
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
