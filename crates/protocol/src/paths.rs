//! Where a hub keeps its files. Takes the environment as a function, to stay free of IO.

use std::path::PathBuf;

/// A debug build keeps its data apart from a release build's, so one run from a checkout can't reach the
/// installed app's hub, or migrate its database.
pub const DIR_NAME: &str = dir_name(cfg!(debug_assertions));
pub const ENV_HOME: &str = "TRESTLE_HOME";
pub const ENV_WORKSPACES: &str = "TRESTLE_WORKSPACES";
pub const ENV_DAEMON_BIN: &str = "TRESTLE_DAEMON_BIN";
pub const ENV_CLAUDE_BIN: &str = "TRESTLE_CLAUDE_BIN";
pub const ENV_LOG: &str = "TRESTLE_LOG";

/// Which hub this is and what it runs. Nothing a worker starts in a workspace gets these, because the
/// code there can be another build of Trestle, and its commands would otherwise reach this hub.
pub const HUB_ONLY_ENV: [&str; 5] = [ENV_HOME, ENV_WORKSPACES, ENV_DAEMON_BIN, ENV_CLAUDE_BIN, ENV_LOG];

const fn dir_name(debug_build: bool) -> &'static str {
    if debug_build { "trestle-dev" } else { "trestle" }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub socket: PathBuf,
    pub lock: PathBuf,
    /// Which process holds the lock, for a client that can't ask it.
    pub pid: PathBuf,
    pub database: PathBuf,
    pub log: PathBuf,
    pub instances_dir: PathBuf,
    pub workspaces_root: PathBuf,
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
        pid: data_dir.join("hub.pid"),
        database: data_dir.join("hub.db"),
        log: data_dir.join("hub.log"),
        instances_dir: data_dir.join("instances"),
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
        assert_eq!(dir_name(false), "trestle");
        assert_eq!(dir_name(true), "trestle-dev");
        assert_eq!(DIR_NAME, dir_name(cfg!(debug_assertions)));
    }

    #[test]
    fn a_socket_path_the_os_would_reject_is_an_error_up_front() {
        let long = format!("/{}", "x".repeat(120));
        assert!(resolve(env_of(&[("HOME", "/h"), (ENV_HOME, &long)])).is_err());
    }
}
