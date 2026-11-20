//! Git, the page of who Roundhouse is on GitHub. Added has the accounts the person gave the hub, by signing in with
//! GitHub, with a token, as an app of your own's installation, or as `gh`'s login on the hub's machine, one of which
//! can be the default, which the hub reads GitHub as and hands to agents on the providers it sets up GitHub on, and
//! each app installed nowhere yet. Who agents are on a provider whose machines are their owner's is on Providers. The
//! page shows the hub's records and settings as they are: which is the
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
