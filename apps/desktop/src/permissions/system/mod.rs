//! What the system says of the app's permissions, and has it ask the person. The app as it runs is given the machine's
//! own, and a test one that says what the test has set and notes what was asked. An app given none, such as one a
//! picture is made of, asks the system nothing.

#[cfg(any(test, feature = "testing"))]
mod fake;
mod given;
#[cfg(target_os = "macos")]
mod mac;
mod none;
#[cfg(target_os = "macos")]
mod notifications;

#[cfg(any(test, feature = "testing"))]
pub use fake::{asked_for, opened_for, read_again, set_standing};
pub(crate) use given::System;
pub(super) use given::{ask, ask_for_badges, open_settings, read};
