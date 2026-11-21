//! What a hub installed as a service is, whichever service manager runs it.

use std::path::PathBuf;

/// What the service runs the daemon with, which tells the hub the service started it.
pub(crate) const ARGUMENTS: [&str; 2] = ["hub", "--service"];

/// What the service runs, as whom, where, and with what environment.
#[cfg_attr(windows, expect(dead_code, reason = "Windows registers no service yet, so nothing reads it"))]
pub(crate) struct Service {
    pub(crate) user: String,
    /// The daemon, through the service's link to the build it runs, so placing a new build moves the service to it.
    pub(crate) program: PathBuf,
    /// The hub's data directory, so the hub never holds the directory the install ran in.
    pub(crate) working_dir: PathBuf,
    /// Where the hub's output goes, which is the log a hub a client starts writes too.
    pub(crate) log: PathBuf,
    pub(crate) environment: Vec<(&'static str, String)>,
}

/// A service whose paths and values have what each service manager's files escape: a space, `&`, `%` and a quote.
#[cfg(test)]
pub(crate) fn escaping() -> Service {
    let data = PathBuf::from("/Users/a & b/Library/Application Support/roundhouse");
    Service {
        user: "agent".to_owned(),
        program: data.join("service/current/roundhoused"),
        working_dir: data.clone(),
        log: data.join("hub.log"),
        environment: vec![("HOME", "/Users/a & b".to_owned()), ("PATH", "/usr/bin:/opt/100%\"x\"".to_owned())],
    }
}
