//! The daemon, with a `main` of its own that gives it what a test says in its environment: timers, where GitHub is, the
//! build it is, and the test to stop with. Everything else about it is the product's.

use std::process::ExitCode;

use daemon::Config;
use process::testing::TestProcess;
use protocol::{Build, StampedBuild};

fn main() -> anyhow::Result<ExitCode> {
    let var = |name: &str| std::env::var(name).ok();
    let build = var(protocol::testing::ENV_TEST_BUILD)
        .and_then(|version| StampedBuild::new(version.leak(), 1))
        .map_or(Build::UNSTAMPED, Build::from);
    daemon::run(Config {
        hub_timers: hub::Timers::from_env(var),
        worker_timers: worker::Timers::from_env(var),
        github: hub::GithubConfig::from_env(var),
        endpoint_ports: match var(protocol::testing::ENV_TEST_ENDPOINT_PORTS) {
            Some(first) => first
                .parse()
                .ok()
                .and_then(hub::EndpointPorts::for_a_test)
                .ok_or_else(|| anyhow::anyhow!("{first} isn't the first of a test hub's ports"))?,
            None => hub::EndpointPorts::PRODUCT,
        },
        relays: if var(protocol::testing::ENV_TEST_NO_RELAYS).is_some() { hub::Relays::None } else { hub::Relays::N0 },
        stops_with: Box::pin(TestProcess::until_gone(TestProcess::from_env(var))),
        also_forgotten: &protocol::testing::ENV_TEST,
        ..Config::new(build)
    })
}
