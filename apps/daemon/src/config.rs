//! What a `main` gives the daemon.

use std::pin::Pin;

/// What the daemon is given as it starts, beside its arguments and what it reads of its environment. The product's is
/// [`Config::new`], and a test's daemon has others, which the rest of the daemon can't tell from the product's.
pub struct Config {
    /// Which build it is, from the stamp its `main` was compiled with, which the hub and the worker are given.
    pub build: protocol::Build,
    pub hub_timers: hub::Timers,
    pub worker_timers: worker::Timers,
    /// Where the hub calls GitHub.
    pub github: hub::GithubConfig,
    /// Which relays the endpoint the hub's workers dial uses, which a test's hub turns off.
    pub relays: hub::Relays,
    /// What a hub, or a worker that can outlive its hub while it has none, stops with, besides being told to stop.
    pub stops_with: StopsWith,
    /// The variables a worker forgets, besides the hub's own, so nothing in a workspace gets them.
    pub also_forgotten: &'static [&'static str],
}

/// Ready once whatever is given it is to stop.
pub type StopsWith = Pin<Box<dyn Future<Output = ()> + Send>>;

impl Config {
    /// The product's, for `build`.
    pub fn new(build: protocol::Build) -> Self {
        Self {
            build,
            hub_timers: hub::Timers::DEFAULT,
            worker_timers: worker::Timers::DEFAULT,
            github: hub::GithubConfig::github(),
            relays: hub::Relays::N0,
            stops_with: Box::pin(std::future::pending()),
            also_forgotten: &[],
        }
    }
}
