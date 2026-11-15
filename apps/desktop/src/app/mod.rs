//! The window: the sidebar on the left and the selected workspace beside it. Shows the hub's records,
//! which client-core keeps current, and is the only place that connects and reconnects.

mod build_line;
mod corner;
mod dictating;
mod drafts;
mod focus;
mod home;
mod hub_source;
mod menu_button;
mod notifying;
mod pull_requests;
mod quitting;
mod reading;
mod records;
mod restarts;
mod settings;
mod shortcuts;
mod title_bar;
mod view;
mod window_buttons;
mod workspaces;

pub use build_line::{Agents, BuildLine, Updating};
pub(crate) use dictating::HearsTheKeyLetGo;
pub use hub_source::HubSource;
pub use quitting::Quitting;
pub(crate) use quitting::quit;
#[cfg(feature = "testing")]
pub use quitting::the_keeping_wait_is;
pub use restarts::UpdateQuestion;
pub(crate) use title_bar::TOP_STRIP_HEIGHT;
pub use title_bar::TitleBar;
#[cfg(feature = "testing")]
pub use title_bar::the_title_bar_is;

#[cfg(test)]
mod tests;

mod app_view;
pub(crate) use app_view::wrapped_step;
pub use app_view::{AppView, Connection};
#[cfg(feature = "testing")]
pub use app_view::{LOOK_AGAIN, SOUNDS_APART};
