//! Where a hub keeps its files. Takes the environment as a function, to stay free of IO.

use std::path::PathBuf;

use crate::StampedBuild;

/// A debug build keeps its data apart from a release build's, so one run from a checkout can't reach the
/// installed app's hub, or migrate its database.
pub const DIR_NAME: &str = dir_name(cfg!(debug_assertions));
pub const ENV_HOME: &str = "ROUNDHOUSE_HOME";
pub const ENV_WORKSPACES: &str = "ROUNDHOUSE_WORKSPACES";
pub const ENV_DAEMON_BIN: &str = "ROUNDHOUSE_DAEMON_BIN";
pub const ENV_CLAUDE_BIN: &str = "ROUNDHOUSE_CLAUDE_BIN";
pub const ENV_LOG: &str = "ROUNDHOUSE_LOG";
/// The TCP port for paired devices, in place of [`DEFAULT_PORT`]. 0 lets the OS choose, which tests use.
pub const ENV_PORT: &str = "ROUNDHOUSE_PORT";
/// How many milliseconds a pairing code works for, in place of ten minutes, so a test can watch one expire. Only a
/// hub built with its `testing` feature reads it.
pub const ENV_PAIRING_LIFE_MS: &str = "ROUNDHOUSE_PAIRING_LIFE_MS";
/// Set to anything, a hub listens for devices on loopback only, so tests never have to get past a firewall. Only a
/// hub built with its `testing` feature reads it.
pub const ENV_LISTEN_ON_LOOPBACK: &str = "ROUNDHOUSE_LISTEN_ON_LOOPBACK";

/// The binary that runs as a hub or a worker. It gives this name in `--version` too.
pub const DAEMON_NAME: &str = "roundhoused";

/// Which hub this is, what it runs and how it listens. Nothing a worker starts in a workspace gets these, because
/// the code there can be another build of Roundhouse, and its commands would otherwise reach this hub.
pub const HUB_ONLY_ENV: [&str; 8] = [
    ENV_HOME,
    ENV_WORKSPACES,
    ENV_DAEMON_BIN,
    ENV_CLAUDE_BIN,
    ENV_LOG,
    ENV_PORT,
    ENV_PAIRING_LIFE_MS,
    ENV_LISTEN_ON_LOOPBACK,
];

/// Where a hub listens for paired devices, while it does. A debug build uses the next port, for the same reason it
/// has its own data: a hub run from a checkout that a phone pairs with listens from then on, and would otherwise
/// take the installed hub's port.
pub const DEFAULT_PORT: u16 = default_port(cfg!(debug_assertions));

const fn dir_name(debug_build: bool) -> &'static str {
    if debug_build { "roundhouse-dev" } else { "roundhouse" }
}

const fn default_port(debug_build: bool) -> u16 {
    if debug_build { 7868 } else { 7867 }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub socket: PathBuf,
    pub lock: PathBuf,
    pub database: PathBuf,
    /// Copies of the database from before each migration, so an older build has one it can open.
    pub backups_dir: PathBuf,
    pub log: PathBuf,
    pub instances_dir: PathBuf,
    /// A copy of the daemon for each stamped build that has started a hub here. See [`Paths::hub_copy`].
    pub daemons_dir: PathBuf,
    pub workspaces_root: PathBuf,
    /// The hub's TLS certificate for paired devices, as DER, made the first time one is needed.
    pub certificate: PathBuf,
    /// Its private key, as PKCS #8 DER.
    pub certificate_key: PathBuf,
}

/// The port a hub listens on for paired devices.
pub fn network_port(env: impl Fn(&str) -> Option<String>) -> Result<u16, String> {
    match env(ENV_PORT).filter(|v| !v.is_empty()) {
        Some(port) => port.parse().map_err(|_| format!("{ENV_PORT} has to be a port number, not {port}")),
        None => Ok(DEFAULT_PORT),
    }
}

impl Paths {
    /// The copy of the daemon that `build`'s hubs run from, in a directory named for the build.
    pub fn hub_copy(&self, build: &StampedBuild) -> PathBuf {
        self.daemons_dir.join(format!("{}-{}", build.version(), build.number())).join(DAEMON_NAME)
    }
}

/// macOS limits a socket path to 103 bytes.
pub const MAX_SOCKET_PATH_BYTES: usize = 103;

pub fn resolve(env: impl Fn(&str) -> Option<String>) -> Result<Paths, String> {
    let home = env("HOME").filter(|h| !h.is_empty()).map(PathBuf::from);
    let data_dir = if let Some(dir) = env(ENV_HOME).filter(|v| !v.is_empty()) {
        PathBuf::from(dir)
    } else {
        let home = home.clone().ok_or("HOME is not set")?;
        if cfg!(target_os = "macos") {
            home.join("Library/Application Support").join(DIR_NAME)
        } else {
            match env("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
                Some(dir) => PathBuf::from(dir).join(DIR_NAME),
                None => home.join(".local/share").join(DIR_NAME),
            }
        }
    };
    let workspaces_root = match env(ENV_WORKSPACES).filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => home.ok_or("HOME is not set")?.join(DIR_NAME).join("workspaces"),
    };
    let socket = data_dir.join("hub.sock");
    if socket.as_os_str().len() > MAX_SOCKET_PATH_BYTES {
        return Err(format!(
            "socket path {} is longer than the {MAX_SOCKET_PATH_BYTES} bytes the OS allows. Set {ENV_HOME} to a shorter path",
            socket.display()
        ));
    }
    Ok(Paths {
        socket,
        lock: data_dir.join("hub.lock"),
        database: data_dir.join("hub.db"),
        backups_dir: data_dir.join("backups"),
        log: data_dir.join("hub.log"),
        instances_dir: data_dir.join("instances"),
        daemons_dir: data_dir.join("daemons"),
        certificate: data_dir.join("hub-cert.der"),
        certificate_key: data_dir.join("hub-key.der"),
        workspaces_root,
        data_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| vars.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
    }

    #[test]
    fn overrides_win_over_defaults() {
        let paths = resolve(env_of(&[("HOME", "/h"), (ENV_HOME, "/d"), (ENV_WORKSPACES, "/w")])).unwrap();
        assert_eq!(paths.socket, PathBuf::from("/d/hub.sock"));
        assert_eq!(paths.workspaces_root, PathBuf::from("/w"));
    }

    #[test]
    fn workspaces_default_to_a_visible_folder_in_home() {
        let paths = resolve(env_of(&[("HOME", "/h")])).unwrap();
        assert_eq!(paths.workspaces_root, PathBuf::from(format!("/h/{DIR_NAME}/workspaces")));
    }

    #[test]
    fn only_a_release_build_defaults_to_the_real_data() {
        assert_eq!(dir_name(false), "roundhouse");
        assert_eq!(dir_name(true), "roundhouse-dev");
        assert_eq!(DIR_NAME, dir_name(cfg!(debug_assertions)));
    }

    #[test]
    fn a_hub_copy_is_named_for_its_build_version_and_number() {
        let paths = resolve(env_of(&[("HOME", "/h"), (ENV_HOME, "/d")])).unwrap();

        let copy = paths.hub_copy(&StampedBuild::new("abc123", 7).unwrap());

        assert_eq!(copy, PathBuf::from("/d/daemons/abc123-7/roundhoused"));
    }

    #[test]
    fn a_debug_build_listens_apart_from_a_release_build_unless_told_where() {
        assert_ne!(default_port(false), default_port(true));
        assert_eq!(network_port(env_of(&[])), Ok(default_port(cfg!(debug_assertions))));
        assert_eq!(network_port(env_of(&[(ENV_PORT, "0")])), Ok(0));
        assert!(network_port(env_of(&[(ENV_PORT, "seventy")])).is_err());
    }

    #[test]
    fn a_socket_path_the_os_would_reject_is_an_error_up_front() {
        let long = format!("/{}", "x".repeat(120));
        assert!(resolve(env_of(&[("HOME", "/h"), (ENV_HOME, &long)])).is_err());
    }
}
