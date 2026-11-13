//! Keeps the app up to date through Velopack, which packages and installs it. It looks for an update on launch, every
//! hour and whenever someone checks, and downloads it in the background. It installs it when someone restarts the app
//! to update it, or else when the app quits, without relaunching. This is the one module that knows about Velopack, so
//! replacing it touches nothing else. A build that Velopack didn't install, such as `cargo run` or
//! `tools/install.sh`, doesn't update.

mod answer;
mod location;
mod problem;
#[cfg(test)]
mod serving;
#[cfg(test)]
mod signing;
mod source;

pub(crate) use answer::Answer;
#[cfg(test)]
use gpui_kit::Context;
#[cfg(test)]
use problem::OutOfReach;
pub(crate) use problem::Said;
use problem::problem;
pub use problem::{Kind, Problem, Refusal, Step};
#[cfg(test)]
use source::{SignedSource, UpdateFeedKey};
#[cfg(test)]
use std::time::Duration;
#[cfg(test)]
use velopack::UpdateManager;

#[cfg(test)]
mod tests;

mod updater;

#[cfg(test)]
pub(crate) use updater::{answered, checking, problem_shown};
#[cfg(test)]
use updater::{apply, keep_looking, keep_up_to_date, newest, options};
pub use updater::{check_now, run_hooks, start};
pub(crate) use updater::{observe, ready, restart_to_update, said};
#[cfg(feature = "testing")]
pub use updater::{show_checking, show_problem, show_ready, show_up_to_date};
