//! The system permissions the app uses on a Mac, how each stands, and what fixes one that isn't allowed, for
//! Settings' Permissions page, as `docs/spec/settings.md` says. Also the Dock's badge, which a Mac shows only once the
//! app's notifications are allowed with badges, so the app asks for those once notifications are allowed.

mod permission;
mod standings;
mod system;

pub(crate) use permission::{Badges, HAS_PERMISSIONS};
pub use permission::{Permission, Standing};
pub(crate) use standings::{ask_for_badges, fix, read, standing};
pub(crate) use system::System;
#[cfg(any(test, feature = "testing"))]
pub use system::{asked_for, opened_for, read_again, set_standing};
