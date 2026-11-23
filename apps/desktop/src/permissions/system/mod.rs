//! What the system says of the app's permissions, and has it ask the person, the same functions on each platform: a
//! Mac's, the nothing elsewhere has, and a test's, which says what the test has set and notes what was asked.

#[cfg(all(not(target_os = "macos"), not(any(test, feature = "testing"))))]
mod elsewhere;
#[cfg(any(test, feature = "testing"))]
mod fake;
#[cfg(all(target_os = "macos", not(any(test, feature = "testing"))))]
mod mac;
#[cfg(all(target_os = "macos", not(any(test, feature = "testing"))))]
mod notifications;

#[cfg(all(not(target_os = "macos"), not(any(test, feature = "testing"))))]
pub(super) use elsewhere::{ask, ask_for_badges, open_settings, read};
#[cfg(any(test, feature = "testing"))]
pub(super) use fake::{ask, ask_for_badges, open_settings, read};
#[cfg(any(test, feature = "testing"))]
pub use fake::{asked_for, opened_for, read_again, set_standing};
#[cfg(all(target_os = "macos", not(any(test, feature = "testing"))))]
pub(super) use mac::{ask, ask_for_badges, open_settings, read};
