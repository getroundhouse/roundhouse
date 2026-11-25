//! MCP servers, the page of the tool servers the hub has of its own. Added lists them, each from the hub's record of it,
//! and the page adds one, signs in to it and removes it. The page shows the hub's records as they are: what a server
//! is called, where it is and how it knows the person are the hub's to say.

use super::{Asking, Body, OfHub, Page, PageId, Within};

mod added;
mod adding;
mod kept;
mod panels;
mod server_menu;
mod servers_page;
mod sign_in;
mod sign_in_panel;
mod words;

use servers_page::Drawing;
pub(super) use servers_page::page;
