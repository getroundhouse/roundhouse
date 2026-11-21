//! Where things are in a workspace's sandbox, and how its worker runs there.

use std::time::Duration;

/// The script that sets a stock image up for a workspace, as [`setup.sh`](./setup.sh) says.
pub(crate) const SETUP: &str = include_str!("setup.sh");

/// Written once the setup script has finished, so a sandbox whose setup was cut short is set up again.
pub(crate) const SET_UP: &str = "/opt/roundhouse/set-up";

/// The worker the hub gave, which every workspace's sandbox runs.
pub(crate) const WORKER: &str = "/opt/roundhouse/roundhoused";

/// The user the worker and its agents run as, whom the setup script makes.
pub(crate) const USER: &str = "roundhouse";
const HOME: &str = "/home/roundhouse";

/// The worker's log in the sandbox. The worker `--attach` starts writes to the stderr it was given, and while it holds
/// the command's own, the sandbox keeps the command running after `--attach` exits, so the hub would never hear its
/// stream end. So both write here, rather than to the hub's log for the workspace, as they do on the local provider.
pub(crate) const WORKER_LOG: &str = "/home/roundhouse/.roundhouse/worker.log";

/// The command that runs the worker's `--attach`, which connects to the sandbox's worker and starts one in a session
/// of its own if none is serving it, so the worker outlives the connection, as it does on the local provider. It's run
/// by a shell, which sends its stderr to [`WORKER_LOG`].
pub(crate) fn worker_command() -> (String, Vec<String>) {
    let script = format!("mkdir -p \"$(dirname {WORKER_LOG})\" && exec \"$0\" \"$@\" 2>> {WORKER_LOG}");
    let args = [
        "-c",
        &script,
        WORKER,
        "worker",
        "--attach",
        "--workspaces-root",
        "/home/roundhouse/workspaces",
        "--state-dir",
        "/home/roundhouse/.roundhouse/instance",
        "--clones-dir",
        "/home/roundhouse/.roundhouse/clones",
    ];
    ("/bin/sh".to_owned(), args.into_iter().map(str::to_owned).collect())
}

/// What the worker runs with, as the user would have from a login: their home, and their agents on the `PATH`.
pub(crate) fn worker_env() -> Vec<(String, String)> {
    let env = [
        ("HOME", HOME),
        ("USER", USER),
        ("LOGNAME", USER),
        ("SHELL", "/bin/bash"),
        ("LANG", "C.UTF-8"),
        ("PATH", "/home/roundhouse/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"),
    ];
    env.into_iter().map(|(key, value)| (key.to_owned(), value.to_owned())).collect()
}

/// Ends what the worker's user runs, the worker and its agents, giving them `grace` to stop by themselves before
/// they're killed, and looking every twentieth of a second for whether they have.
pub(crate) fn end_user_processes(grace: Duration) -> String {
    let looks = grace.as_millis() / 50;
    format!(
        "pkill -TERM -u {USER} || exit 0; for _ in $(seq {looks}); do pgrep -u {USER} > /dev/null || exit 0; \
         sleep 0.05; done; pkill -KILL -u {USER}; exit 0"
    )
}

/// Whether a worker serves the sandbox: the one `--attach` started, which listens on its socket.
pub(crate) const WORKER_RUNNING: &str = "pgrep -u roundhouse -f 'roundhoused worker --listen' > /dev/null";

/// The SHA-256 of the worker in place, or nothing if there's none.
pub(crate) const WORKER_SHA256: &str = "sha256sum /opt/roundhouse/roundhoused 2> /dev/null | cut -d ' ' -f 1";
