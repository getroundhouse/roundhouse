//! What a test gives the daemon for tests, `tools/stand-ins/daemon-for-tests`, in its
//! environment. Only it reads them, so nothing in the product does.

/// Shorter timers for a test's hub, as `hub::Timers` says.
pub const ENV_TEST_HUB_TIMERS: &str = "ROUNDHOUSE_TEST_HUB_TIMERS";
/// Shorter timers for a test's workers, as `worker::Timers` says.
pub const ENV_TEST_WORKER_TIMERS: &str = "ROUNDHOUSE_TEST_WORKER_TIMERS";
/// Where a test's hub calls GitHub, both its API and its device flow, such as a stand-in's address, as
/// `hub::GithubConfig::from_env` says.
pub const ENV_TEST_GITHUB_API: &str = "ROUNDHOUSE_TEST_GITHUB_API";
/// The PID of the test a process was started for, which it stops with once it has gone, as
/// `process::testing::TestProcess` says.
pub const ENV_TEST_PROCESS: &str = "ROUNDHOUSE_TEST_PROCESS";
/// The build a test's daemon says it is, as one from before an update would.
pub const ENV_TEST_BUILD: &str = "ROUNDHOUSE_TEST_BUILD";

/// That a test's hub uses no relay for the endpoint its workers dial, and binds it only to loopback, which keeps the
/// test off the internet.
pub const ENV_TEST_NO_RELAYS: &str = "ROUNDHOUSE_TEST_NO_RELAYS";

/// The first of the ports a test's hub's endpoints bind, as `hub::EndpointPorts::for_a_test` says, which its run gives
/// no other hub.
pub const ENV_TEST_ENDPOINT_PORTS: &str = "ROUNDHOUSE_TEST_ENDPOINT_PORTS";

/// Each of them, none of which anything in a workspace gets.
pub const ENV_TEST: [&str; 7] = [
    ENV_TEST_HUB_TIMERS,
    ENV_TEST_WORKER_TIMERS,
    ENV_TEST_GITHUB_API,
    ENV_TEST_PROCESS,
    ENV_TEST_BUILD,
    ENV_TEST_NO_RELAYS,
    ENV_TEST_ENDPOINT_PORTS,
];
