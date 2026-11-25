//! One program that runs as a hub or a worker. A hub starts its workers by running this same program, so on a Mac
//! they're separate processes from day one. All of it but `main` is this library, as with the desktop app, so the tests
//! run the same code from a `main` of their own, `tools/stand-ins/daemon-for-tests`, which gives it another [`Config`].

mod config;
mod hub_role;
mod preflight;
mod run;
mod runtime;
mod service_builds;
mod service_log;
mod signals;
mod worker_role;

pub use config::{Config, StopsWith};
pub use run::run;
