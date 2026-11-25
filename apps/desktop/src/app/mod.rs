//! The window: the sidebar on the left and the selected workspace beside it. Shows the hub's records,
//! which client-core keeps current, and how client-core's link to the hub stands, giving the link its way to connect.

mod build_line;
mod corner;
pub(crate) mod device_name;
mod drafts;
mod failure;
mod focus;
mod home;
mod hub_source;
pub(crate) mod hubs;
mod keeping_workers;
mod linking;
mod menu_button;
mod notifying;
pub(crate) mod pairing;
mod presentation;
mod pull_requests;
mod quitting;
mod reading;
mod records;
mod remote_failed;
mod restarts;
pub(crate) mod saved_hubs;
mod selecting;
mod selection;
mod settings;
pub(crate) mod share_hub;
mod shortcuts;
mod this_device;
mod title_bar;
mod updating_hub;
mod view;
mod window_buttons;
mod workspaces;

pub use build_line::{Agents, BuildLine, Updating};
#[cfg(any(test, feature = "testing"))]
pub use device_name::the_device_name_is;
pub use failure::{ConnectionFailure, Unconnected};
pub use hub_source::HubSource;
pub use quitting::Quitting;
pub(crate) use quitting::quit;
#[cfg(any(test, feature = "testing"))]
pub use quitting::the_keeping_wait_is;
pub use restarts::UpdateQuestion;
pub(crate) use title_bar::TOP_STRIP;
pub use title_bar::TitleBar;
#[cfg(any(test, feature = "testing"))]
pub use title_bar::the_title_bar_is;

#[cfg(test)]
mod tests;

mod app_view;
pub(crate) use app_view::wrapped_step;
pub use app_view::{AppView, Connection};
#[cfg(any(test, feature = "testing"))]
pub use app_view::{LOOK_AGAIN, SOUNDS_APART};
