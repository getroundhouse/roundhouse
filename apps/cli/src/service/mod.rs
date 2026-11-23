//! Installing a hub as a service on this machine, as `docs/decisions/hub-package.md` says: placing this build's daemon
//! and command line in the data directory of the user the hub runs as, and registering the daemon with the machine's
//! service manager, which starts it at boot, with nobody logged in.

mod definition;
mod installing;
mod manager;
mod placing;
mod service_user;

pub(crate) use installing::{install, uninstall};
pub(crate) use placing::place;
