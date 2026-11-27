//! Where things are in a workspace's sandbox, and how its worker runs there. The adapter makes every sandbox from
//! [`IMAGE`] and sets it up with [`SETUP`], so what this says of it holds for every one.

use std::net::{Ipv4Addr, SocketAddr};

use protocol::dialing::WorkerTicket;

/// The OCI image every sandbox is made from. Not one a person picks, since the setup script, the user and these paths
/// assume it.
pub(crate) const IMAGE: &str = "ubuntu:24.04";

/// The script that sets [`IMAGE`] up for a workspace, as [`setup.sh`](./setup.sh) says.
pub(crate) const SETUP: &str = include_str!("setup.sh");

/// Written once the setup script has finished, so a sandbox whose setup was cut short is set up again.
pub(crate) const SET_UP: &str = "/opt/roundhouse/set-up";

/// The worker the hub gave, which every workspace's sandbox runs.
pub(crate) const WORKER: &str = "/opt/roundhouse/roundhoused";

/// The user the worker and its agents run as, whom the setup script makes.
pub(crate) const USER: &str = "roundhouse";

/// The worker's own log in the sandbox, which `--log` has it write to.
const WORKER_LOG: &str = "/home/roundhouse/.roundhouse/worker.log";

/// The worker's state directory, which `roundhoused end-worker` finds it by.
const STATE_DIR: &str = "/home/roundhouse/.roundhouse/instance";

/// The UDP port the worker binds in the sandbox, which a port on the hub's machine is forwarded to, so the hub opens a
/// direct path to the worker there. UDP from the sandbox out to its host never forms one, as #2451 found.
pub(crate) const WORKER_PORT: u16 = 7900;

/// The arguments that start the worker, in a session of its own, with `ticket`, dialing from [`WORKER_PORT`] and
/// advertising `forwarded`, the host's end of that port, as where the hub can reach it.
pub(crate) fn worker_args(ticket: &WorkerTicket, forwarded: u16) -> Vec<String> {
    let advertise = SocketAddr::from((Ipv4Addr::LOCALHOST, forwarded)).to_string();
    let args = [
        "worker",
        "--ticket",
        ticket.as_str(),
        "--port",
        &WORKER_PORT.to_string(),
        "--advertise",
        &advertise,
        "--log",
        WORKER_LOG,
        "--detach",
        "--workspaces-root",
        "/home/roundhouse/workspaces",
        "--state-dir",
        STATE_DIR,
        "--clones-dir",
        "/home/roundhouse/.roundhouse/clones",
    ];
    args.into_iter().map(str::to_owned).collect()
}

/// What the worker runs with, as the user would have from a login: their home, and their agents on the `PATH`.
pub(crate) fn worker_env() -> Vec<(String, String)> {
    let env = [
        ("HOME", "/home/roundhouse"),
        ("USER", USER),
        ("LOGNAME", USER),
        ("SHELL", "/bin/bash"),
        ("LANG", "C.UTF-8"),
        ("PATH", "/home/roundhouse/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"),
    ];
    env.into_iter().map(|(key, value)| (key.to_owned(), value.to_owned())).collect()
}

/// The script that ends the worker and what it left, such as its agents, as the worker itself finds them, at once if
/// `now`. A sandbox the hub gave no worker yet, as one just made, has none to end, so that's done at once: the hub
/// clears an instance before it starts the first worker there.
pub(crate) fn end_worker_script(now: bool) -> String {
    let now = if now { " --now" } else { "" };
    format!("test -e {WORKER} || exit 0; exec {WORKER} end-worker --state-dir {STATE_DIR}{now}")
}

/// The SHA-256 of the worker in place, as `sha256sum` writes it, or nothing if there's none.
pub(crate) const WORKER_SHA256: &str = "sha256sum /opt/roundhouse/roundhoused 2> /dev/null | cut -d ' ' -f 1";

#[cfg(test)]
mod tests {
    use super::*;

    /// So the worker dials from the port forwarded into the sandbox and has the hub reach it at the host's end, in a
    /// session of its own that outlives the command that started it, with its own log.
    #[test]
    fn the_worker_dials_from_the_forwarded_port_and_advertises_the_hosts_end_of_it() {
        let ticket = WorkerTicket::from("the-ticket".to_string());

        let args = worker_args(&ticket, 51234);

        let after = |flag: &str| args.iter().position(|arg| arg == flag).map(|at| args[at + 1].as_str());
        assert_eq!(after("--ticket"), Some("the-ticket"));
        assert_eq!(after("--port"), Some("7900"));
        assert_eq!(after("--advertise"), Some("127.0.0.1:51234"));
        assert_eq!(after("--log"), Some(WORKER_LOG));
        assert!(args.iter().any(|arg| arg == "--detach"), "{args:?}");
    }

    /// So ending the worker of a sandbox that has none in place yet, as the hub does before it starts the first, is done
    /// rather than failing. No machine that runs this test has the sandbox's worker, so it's that case.
    #[test]
    fn ending_the_worker_where_there_is_none_yet_succeeds() {
        assert!(!std::path::Path::new(WORKER).exists(), "this test needs a machine without {WORKER}");

        for now in [false, true] {
            let ended = std::process::Command::new("/bin/sh").args(["-c", &end_worker_script(now)]).status().unwrap();
            assert!(ended.success(), "{ended:?}");
        }
        assert!(end_worker_script(true).ends_with(&format!("end-worker --state-dir {STATE_DIR} --now")));
    }
}
