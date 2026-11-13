//! The bottom of the sidebar: the build line, with what a check for updates is doing or found in the version's place
//! and the question mark's menu and Settings at its end, and over it where updating has got to, and which checkout a
//! build from one is from.

mod controls;
mod update_card;
mod update_details;

use update_card::update_card;

mod version;

pub(super) use version::footer;
