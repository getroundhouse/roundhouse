//! Settings as it fills the window: a sidebar with a search at its top and a page for each part of the app under a
//! heading, with a way back at its top right, and the chosen page beside it. Most pages are a list of settings, and a
//! page with more to show than a setting's row, such as Agents, draws what's on it itself.

mod account_row;
mod agents;
mod dictation_row;
mod field;
mod general;
mod git;
mod lists;
mod notifications;
mod redraw;
mod rows;
mod sidebar;
#[cfg(test)]
mod tests;

mod settings_view;
pub(crate) use settings_view::{Asking, FIND_KEY, KEY_CONTEXT, OfHub, PageId, page};
use settings_view::{Body, Control, Page, Section, Setting, Within};
#[cfg(test)]
use settings_view::{found, sections, shown};
