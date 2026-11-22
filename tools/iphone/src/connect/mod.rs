//! App Store Connect's API, with an App Store Connect key: the app's record there, whether it has processed a build,
//! and what a build tells testers to try.

mod client;
mod key;
mod notes;
mod processing;
mod record;

pub(crate) use client::Client;
pub(crate) use key::{IssuerId, Key, KeyId};
pub(crate) use notes::{Notes, set_notes};
pub(crate) use processing::wait_for_processing;
pub(crate) use record::find_app;
